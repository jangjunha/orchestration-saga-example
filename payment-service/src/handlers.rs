use anyhow::Result;
use num_traits::ToPrimitive;
use diesel::prelude::*;
use diesel_async::{pooled_connection::bb8::Pool, AsyncPgConnection, RunQueryDsl};
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
            CommandType::ProcessPayment => self.handle_process_payment(&mut conn, &command).await?,
            CommandType::CompensatePayment => self.handle_compensate_payment(&mut conn, &command).await?,
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

    async fn handle_process_payment(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let payment_data: PaymentData = serde_json::from_value(command.payload.clone())?;
        
        let existing_payment = payments::table
            .filter(payments::order_id.eq(payment_data.order_id))
            .first::<Payment>(conn)
            .await
            .optional()?;

        if let Some(payment) = existing_payment {
            if payment.status == "processed" {
                return Ok(CommandReply::success(
                    command.id,
                    command.saga_id,
                    Some(serde_json::to_value(&payment)?),
                ));
            }
        }

        let success_rate = 0.8;
        let should_succeed = rand::random::<f64>() < success_rate;

        if !should_succeed {
            return Ok(CommandReply::failed(
                command.id,
                command.saga_id,
                "Payment processing failed".to_string(),
            ));
        }

        let new_payment = NewPayment {
            id: Uuid::new_v4(),
            order_id: payment_data.order_id,
            amount: bigdecimal::BigDecimal::from(payment_data.amount.to_i64().unwrap()),
            payment_method: payment_data.payment_method,
            status: "processed".to_string(),
        };

        diesel::insert_into(payments::table)
            .values(&new_payment)
            .execute(conn)
            .await?;

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::to_value(&new_payment)?),
        ))
    }

    async fn handle_compensate_payment(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let payment_data: PaymentData = serde_json::from_value(command.payload.clone())?;
        
        let updated_rows = diesel::update(payments::table.filter(payments::order_id.eq(payment_data.order_id)))
            .set(payments::status.eq("refunded"))
            .execute(conn)
            .await?;

        if updated_rows > 0 {
            info!("Payment refunded for order: {}", payment_data.order_id);
        }

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::json!({"refunded": true})),
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