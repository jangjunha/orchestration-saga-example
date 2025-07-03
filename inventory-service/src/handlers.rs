use anyhow::Result;
use diesel::prelude::*;
use diesel_async::{pooled_connection::bb8::Pool, AsyncPgConnection, RunQueryDsl, AsyncConnection};
use futures::StreamExt;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::Message;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;
use shared::*;
use crate::models::*;
use crate::schema::*;

type DbPool = Pool<AsyncPgConnection>;

pub struct CommandHandler {
    pool: DbPool,
    producer: FutureProducer,
    reply_topic: String,
}

impl CommandHandler {
    pub fn new(pool: DbPool, producer: FutureProducer, reply_topic: String) -> Self {
        Self { pool, producer, reply_topic }
    }

    pub async fn run(&self, consumer: StreamConsumer) {
        let mut message_stream = consumer.stream();
        
        while let Some(message) = message_stream.next().await {
            match message {
                Ok(m) => {
                    if let Some(payload) = m.payload_view::<str>() {
                        match payload {
                            Ok(json_str) => {
                                if let Ok(command) = serde_json::from_str::<Command>(json_str) {
                                    if let Err(e) = self.handle_command(command).await {
                                        error!("Error handling command: {}", e);
                                    }
                                }
                            }
                            Err(e) => error!("Error parsing payload: {}", e),
                        }
                    }
                    if let Err(e) = consumer.commit_message(&m, rdkafka::consumer::CommitMode::Async) {
                        error!("Error committing message: {}", e);
                    }
                }
                Err(e) => error!("Error receiving message: {}", e),
            }
        }
    }

    async fn handle_command(&self, command: Command) -> Result<()> {
        let mut conn = self.pool.get().await?;

        if let Some(existing) = self.check_idempotency(&mut conn, &command.idempotency_key).await? {
            info!("Command already processed, returning cached result");
            let reply = CommandReply {
                id: Uuid::new_v4(),
                command_id: command.id,
                saga_id: command.saga_id,
                status: CommandStatus::Success,
                result: existing.result,
                error: None,
                created_at: chrono::Utc::now(),
            };
            self.send_reply(reply).await?;
            return Ok(());
        }

        let reply = match command.command_type {
            CommandType::ReserveInventory => self.handle_reserve_inventory(&mut conn, &command).await?,
            CommandType::CompensateInventory => self.handle_compensate_inventory(&mut conn, &command).await?,
            _ => {
                warn!("Unsupported command type: {:?}", command.command_type);
                CommandReply::failed(
                    command.id,
                    command.saga_id,
                    "Unsupported command type".to_string(),
                )
            }
        };

        self.store_processed_command(&mut conn, &command, &reply).await?;
        self.send_reply(reply).await?;

        Ok(())
    }

    async fn handle_reserve_inventory(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let inventory_data: InventoryData = serde_json::from_value(command.payload.clone())?;
        
        let existing_reservation = reservations::table
            .filter(reservations::order_id.eq(inventory_data.order_id))
            .filter(reservations::product_id.eq(inventory_data.product_id))
            .first::<Reservation>(conn)
            .await
            .optional()?;

        if let Some(reservation) = existing_reservation {
            if reservation.status == "reserved" {
                return Ok(CommandReply::success(
                    command.id,
                    command.saga_id,
                    Some(serde_json::to_value(&reservation)?),
                ));
            }
        }

        let inventory_item = inventory::table
            .filter(inventory::product_id.eq(inventory_data.product_id))
            .first::<Inventory>(conn)
            .await
            .optional()?;

        let inventory_item = match inventory_item {
            Some(item) => item,
            None => {
                return Ok(CommandReply::failed(
                    command.id,
                    command.saga_id,
                    "Product not found".to_string(),
                ));
            }
        };

        if inventory_item.available_quantity < inventory_data.quantity {
            return Ok(CommandReply::failed(
                command.id,
                command.saga_id,
                "Insufficient inventory".to_string(),
            ));
        }

        conn.transaction::<_, anyhow::Error, _>(|conn| {
            Box::pin(async move {
                diesel::update(inventory::table.filter(inventory::product_id.eq(inventory_data.product_id)))
                    .set((
                        inventory::available_quantity.eq(inventory::available_quantity - inventory_data.quantity),
                        inventory::reserved_quantity.eq(inventory::reserved_quantity + inventory_data.quantity),
                    ))
                    .execute(conn)
                    .await?;

                let new_reservation = NewReservation {
                    id: Uuid::new_v4(),
                    product_id: inventory_data.product_id,
                    order_id: inventory_data.order_id,
                    quantity: inventory_data.quantity,
                    status: "reserved".to_string(),
                };

                diesel::insert_into(reservations::table)
                    .values(&new_reservation)
                    .execute(conn)
                    .await?;

                Ok(())
            })
        }).await?;

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::json!({"reserved": true, "quantity": inventory_data.quantity})),
        ))
    }

    async fn handle_compensate_inventory(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let inventory_data: InventoryData = serde_json::from_value(command.payload.clone())?;
        
        let reservation = reservations::table
            .filter(reservations::order_id.eq(inventory_data.order_id))
            .filter(reservations::product_id.eq(inventory_data.product_id))
            .first::<Reservation>(conn)
            .await
            .optional()?;

        if let Some(reservation) = reservation {
            if reservation.status == "reserved" {
                conn.transaction::<_, anyhow::Error, _>(|conn| {
                    Box::pin(async move {
                        diesel::update(inventory::table.filter(inventory::product_id.eq(inventory_data.product_id)))
                            .set((
                                inventory::available_quantity.eq(inventory::available_quantity + reservation.quantity),
                                inventory::reserved_quantity.eq(inventory::reserved_quantity - reservation.quantity),
                            ))
                            .execute(conn)
                            .await?;

                        diesel::update(reservations::table.filter(reservations::id.eq(reservation.id)))
                            .set(reservations::status.eq("cancelled"))
                            .execute(conn)
                            .await?;

                        Ok(())
                    })
                }).await?;

                info!("Inventory reservation cancelled for order: {}", inventory_data.order_id);
            }
        }

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::json!({"compensated": true})),
        ))
    }

    async fn check_idempotency(&self, conn: &mut AsyncPgConnection, key: &str) -> Result<Option<ProcessedCommand>> {
        let result = processed_commands::table
            .filter(processed_commands::idempotency_key.eq(key))
            .first::<ProcessedCommand>(conn)
            .await
            .optional()?;
        Ok(result)
    }

    async fn store_processed_command(&self, conn: &mut AsyncPgConnection, command: &Command, reply: &CommandReply) -> Result<()> {
        let processed_command = ProcessedCommand {
            idempotency_key: command.idempotency_key.clone(),
            command_id: command.id,
            result: reply.result.clone(),
            processed_at: Some(chrono::Utc::now()),
        };

        diesel::insert_into(processed_commands::table)
            .values(&processed_command)
            .execute(conn)
            .await?;

        Ok(())
    }

    async fn send_reply(&self, reply: CommandReply) -> Result<()> {
        let json = serde_json::to_string(&reply)?;
        let key = reply.saga_id.to_string();
        let record = FutureRecord::to(&self.reply_topic)
            .payload(&json)
            .key(&key);

        self.producer.send(record, Duration::from_secs(5)).await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to send reply: {}", e))?;

        Ok(())
    }
}