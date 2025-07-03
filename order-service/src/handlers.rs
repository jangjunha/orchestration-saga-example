use anyhow::Result;
use num_traits::ToPrimitive;
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
            CommandType::CreateOrder => self.handle_create_order(&mut conn, &command).await?,
            CommandType::ApproveOrder => self.handle_approve_order(&mut conn, &command).await?,
            CommandType::CancelOrder => self.handle_cancel_order(&mut conn, &command).await?,
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

    async fn handle_create_order(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let order_data: OrderData = serde_json::from_value(command.payload.clone())?;
        
        let new_order = NewOrder {
            id: order_data.order_id,
            customer_id: order_data.customer_id,
            product_id: order_data.product_id,
            quantity: order_data.quantity,
            total_amount: bigdecimal::BigDecimal::from(order_data.total_amount.to_i64().unwrap()),
            status: "created".to_string(),
        };

        let order_data_clone = order_data.clone();
        conn.transaction::<_, anyhow::Error, _>(|conn| {
            Box::pin(async move {
                diesel::insert_into(orders::table)
                    .values(&new_order)
                    .execute(conn)
                    .await?;

                let outbox_event = NewOutboxEvent {
                    id: Uuid::new_v4(),
                    aggregate_id: order_data_clone.order_id,
                    event_type: "OrderCreated".to_string(),
                    event_data: serde_json::to_value(&order_data_clone)?,
                };

                diesel::insert_into(outbox_events::table)
                    .values(&outbox_event)
                    .execute(conn)
                    .await?;

                Ok(())
            })
        }).await?;

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::to_value(&order_data)?),
        ))
    }

    async fn handle_approve_order(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let order_data: OrderData = serde_json::from_value(command.payload.clone())?;
        
        diesel::update(orders::table.filter(orders::id.eq(order_data.order_id)))
            .set(orders::status.eq("approved"))
            .execute(conn)
            .await?;

        info!("Order {} approved", order_data.order_id);

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::to_value(&order_data)?),
        ))
    }

    async fn handle_cancel_order(&self, conn: &mut AsyncPgConnection, command: &Command) -> Result<CommandReply> {
        let order_data: OrderData = serde_json::from_value(command.payload.clone())?;
        
        diesel::update(orders::table.filter(orders::id.eq(order_data.order_id)))
            .set(orders::status.eq("cancelled"))
            .execute(conn)
            .await?;

        info!("Order {} cancelled", order_data.order_id);

        Ok(CommandReply::success(
            command.id,
            command.saga_id,
            Some(serde_json::to_value(&order_data)?),
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

pub struct SagaManager {
    pool: DbPool,
    producer: FutureProducer,
}

impl SagaManager {
    pub fn new(pool: DbPool, producer: FutureProducer) -> Self {
        Self { pool, producer }
    }

    pub async fn run_reply_handler(&self, consumer: StreamConsumer) {
        let mut message_stream = consumer.stream();
        
        while let Some(message) = message_stream.next().await {
            match message {
                Ok(m) => {
                    if let Some(payload) = m.payload_view::<str>() {
                        match payload {
                            Ok(json_str) => {
                                if let Ok(reply) = serde_json::from_str::<CommandReply>(json_str) {
                                    if let Err(e) = self.handle_reply(reply).await {
                                        error!("Error handling reply: {}", e);
                                    }
                                }
                            }
                            Err(e) => error!("Error parsing reply payload: {}", e),
                        }
                    }
                    if let Err(e) = consumer.commit_message(&m, rdkafka::consumer::CommitMode::Async) {
                        error!("Error committing reply message: {}", e);
                    }
                }
                Err(e) => error!("Error receiving reply message: {}", e),
            }
        }
    }

    async fn handle_reply(&self, reply: CommandReply) -> Result<()> {
        let mut conn = self.pool.get().await?;
        
        // Load the saga from database
        let saga_data = saga_transactions::table
            .filter(saga_transactions::id.eq(reply.saga_id))
            .first::<crate::models::DbSagaTransaction>(&mut conn)
            .await?;

        let mut saga = SagaTransaction::try_from(saga_data)?;
        
        match reply.status {
            CommandStatus::Success => {
                info!("Command {} succeeded for saga {}", reply.command_id, reply.saga_id);
                
                // Check if we're in compensation mode
                if saga.status == shared::SagaStatus::Compensating {
                    // Move to next compensation step
                    if let Some(compensation_index_val) = saga.context.get("compensation_index") {
                        let compensation_index: usize = serde_json::from_value(compensation_index_val.clone())?;
                        saga.context.insert("compensation_index".to_string(), serde_json::to_value(compensation_index + 1)?);
                        
                        // Process next compensation step
                        self.process_next_compensation(&mut saga).await?;
                    } else {
                        // No compensation tracking, mark as completed
                        saga.status = shared::SagaStatus::Compensated;
                        info!("Saga {} compensation completed successfully", saga.id);
                    }
                } else {
                    // Normal forward flow
                    saga.advance_step();
                    
                    // Try to process next step
                    if let Some(step) = saga.next_step().cloned() {
                        let command = self.create_command_for_step(&saga, &step)?;
                        self.send_command(&command, &step.service_name).await?;
                        info!("Sent command {} to {} for saga {}", command.id, step.service_name, saga.id);
                    } else {
                        // Saga completed successfully
                        saga.status = shared::SagaStatus::Completed;
                        info!("Saga {} completed successfully", saga.id);
                    }
                }
            }
            CommandStatus::Failed => {
                error!("Command {} failed for saga {}: {:?}", reply.command_id, reply.saga_id, reply.error);
                saga.status = shared::SagaStatus::Compensating;
                // Start compensation process
                self.start_compensation(&mut saga).await?;
            }
            CommandStatus::Compensated => {
                info!("Command {} compensated for saga {}", reply.command_id, reply.saga_id);
                // Continue compensation if needed
                self.continue_compensation(&mut saga).await?;
            }
        }
        
        // Update saga in database
        let updated_saga = crate::models::DbSagaTransaction::from(saga);
        diesel::update(saga_transactions::table.filter(saga_transactions::id.eq(reply.saga_id)))
            .set(&updated_saga)
            .execute(&mut conn)
            .await?;
        
        Ok(())
    }

    async fn start_compensation(&self, saga: &mut SagaTransaction) -> Result<()> {
        let compensation_steps = saga.get_compensation_steps();
        
        // Convert to owned values to store in context
        let owned_steps: Vec<SagaStep> = compensation_steps.into_iter().cloned().collect();
        
        // Store all compensation commands to process them in sequence
        saga.context.insert("compensation_steps".to_string(), serde_json::to_value(&owned_steps)?);
        saga.context.insert("compensation_index".to_string(), serde_json::to_value(0)?);
        
        // Start with the first compensation step
        self.process_next_compensation(saga).await?;
        
        Ok(())
    }

    async fn process_next_compensation(&self, saga: &mut SagaTransaction) -> Result<()> {
        let compensation_steps: Vec<SagaStep> = serde_json::from_value(
            saga.context.get("compensation_steps").unwrap().clone()
        )?;
        let compensation_index: usize = serde_json::from_value(
            saga.context.get("compensation_index").unwrap().clone()
        )?;

        if let Some(step) = compensation_steps.get(compensation_index) {
            if let Some(compensation_type) = &step.compensation_type {
                let payload = match compensation_type {
                    CommandType::CancelOrder => {
                        let order_data: OrderData = serde_json::from_value(
                            saga.context.get("order_data").unwrap().clone()
                        )?;
                        serde_json::to_value(order_data)?
                    }
                    CommandType::CompensatePayment => {
                        let order_data: OrderData = serde_json::from_value(
                            saga.context.get("order_data").unwrap().clone()
                        )?;
                        let payment_data = PaymentData {
                            order_id: order_data.order_id,
                            amount: order_data.total_amount,
                            payment_method: "credit_card".to_string(),
                        };
                        serde_json::to_value(payment_data)?
                    }
                    CommandType::CompensateInventory => {
                        let order_data: OrderData = serde_json::from_value(
                            saga.context.get("order_data").unwrap().clone()
                        )?;
                        let inventory_data = InventoryData {
                            product_id: order_data.product_id,
                            quantity: order_data.quantity,
                            order_id: order_data.order_id,
                        };
                        serde_json::to_value(inventory_data)?
                    }
                    _ => saga.context.get("order_data").unwrap().clone(),
                };
                
                let compensation_command = Command::new(
                    saga.id,
                    compensation_type.clone(),
                    payload,
                );
                self.send_command(&compensation_command, &step.service_name).await?;
                info!("Started compensation step {} for saga {}", compensation_index, saga.id);
            }
        } else {
            // No more compensation steps
            saga.status = shared::SagaStatus::Compensated;
            info!("All compensations completed for saga {}", saga.id);
        }
        Ok(())
    }

    async fn continue_compensation(&self, saga: &mut SagaTransaction) -> Result<()> {
        let compensation_steps = saga.get_compensation_steps();
        // This is a simplified compensation flow
        // In a real implementation, you'd track which compensations have been completed
        if compensation_steps.is_empty() {
            saga.status = shared::SagaStatus::Compensated;
            info!("Compensation completed for saga {}", saga.id);
        }
        Ok(())
    }

    pub async fn start_saga(&self, mut saga: SagaTransaction) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let db_saga = DbSagaTransaction::from(saga.clone());
        diesel::insert_into(saga_transactions::table)
            .values(&db_saga)
            .execute(&mut conn)
            .await?;

        let step_option = {
            let saga_ref = &mut saga;
            saga_ref.next_step().cloned()
        };
        
        if let Some(step) = step_option {
            let command = self.create_command_for_step(&saga, &step)?;
            self.send_command(&command, &step.service_name).await?;
        }

        Ok(())
    }

    fn create_command_for_step(&self, saga: &SagaTransaction, step: &SagaStep) -> Result<Command> {
        let payload = match step.command_type {
            CommandType::CreateOrder | CommandType::ApproveOrder | CommandType::CancelOrder => {
                let order_data: OrderData = serde_json::from_value(
                    saga.context.get("order_data").unwrap().clone()
                )?;
                serde_json::to_value(order_data)?
            }
            CommandType::ProcessPayment => {
                let order_data: OrderData = serde_json::from_value(
                    saga.context.get("order_data").unwrap().clone()
                )?;
                let payment_data = PaymentData {
                    order_id: order_data.order_id,
                    amount: order_data.total_amount,
                    payment_method: "credit_card".to_string(),
                };
                serde_json::to_value(payment_data)?
            }
            CommandType::ReserveInventory => {
                let order_data: OrderData = serde_json::from_value(
                    saga.context.get("order_data").unwrap().clone()
                )?;
                let inventory_data = InventoryData {
                    product_id: order_data.product_id,
                    quantity: order_data.quantity,
                    order_id: order_data.order_id,
                };
                serde_json::to_value(inventory_data)?
            }
            _ => return Err(anyhow::anyhow!("Unsupported command type")),
        };

        Ok(Command::new(saga.id, step.command_type.clone(), payload))
    }

    async fn send_command(&self, command: &Command, service_name: &str) -> Result<()> {
        let topic = format!("{}-commands", service_name);
        let json = serde_json::to_string(command)?;
        let key = command.saga_id.to_string();
        let record = FutureRecord::to(&topic)
            .payload(&json)
            .key(&key);

        self.producer.send(record, Duration::from_secs(5)).await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to send command: {}", e))?;

        Ok(())
    }
}