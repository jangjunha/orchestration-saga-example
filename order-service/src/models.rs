use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use shared::*;

#[derive(Debug, Clone, Queryable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::orders)]
pub struct Order {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub total_amount: bigdecimal::BigDecimal,
    pub status: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = crate::schema::orders)]
pub struct NewOrder {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub total_amount: bigdecimal::BigDecimal,
    pub status: String,
}

#[derive(Debug, Clone, Queryable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::outbox_events)]
pub struct DbOutboxEvent {
    pub id: Uuid,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub processed: Option<bool>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = crate::schema::outbox_events)]
pub struct NewOutboxEvent {
    pub id: Uuid,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub event_data: serde_json::Value,
}

#[derive(Debug, Clone, Queryable, Insertable, AsChangeset, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::saga_transactions)]
pub struct DbSagaTransaction {
    pub id: Uuid,
    pub steps: serde_json::Value,
    pub current_step: i32,
    pub status: String,
    pub context: serde_json::Value,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::processed_commands)]
pub struct ProcessedCommand {
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub result: Option<serde_json::Value>,
    pub processed_at: Option<DateTime<Utc>>,
}

impl From<SagaTransaction> for DbSagaTransaction {
    fn from(saga: SagaTransaction) -> Self {
        Self {
            id: saga.id,
            steps: serde_json::to_value(saga.steps).unwrap(),
            current_step: saga.current_step as i32,
            status: format!("{:?}", saga.status),
            context: serde_json::to_value(saga.context).unwrap(),
            created_at: Some(saga.created_at),
            updated_at: Some(saga.updated_at),
        }
    }
}

impl TryFrom<DbSagaTransaction> for SagaTransaction {
    type Error = anyhow::Error;

    fn try_from(db_saga: DbSagaTransaction) -> Result<Self, Self::Error> {
        let steps: Vec<SagaStep> = serde_json::from_value(db_saga.steps)?;
        let status = match db_saga.status.as_str() {
            "Started" => SagaStatus::Started,
            "InProgress" => SagaStatus::InProgress,
            "Completed" => SagaStatus::Completed,
            "Compensating" => SagaStatus::Compensating,
            "Compensated" => SagaStatus::Compensated,
            "Failed" => SagaStatus::Failed,
            _ => SagaStatus::Failed,
        };
        let context = serde_json::from_value(db_saga.context)?;

        Ok(Self {
            id: db_saga.id,
            steps,
            current_step: db_saga.current_step as usize,
            status,
            context,
            created_at: db_saga.created_at.unwrap_or_else(|| Utc::now()),
            updated_at: db_saga.updated_at.unwrap_or_else(|| Utc::now()),
        })
    }
}

impl From<OutboxEvent> for DbOutboxEvent {
    fn from(event: OutboxEvent) -> Self {
        Self {
            id: event.id,
            aggregate_id: event.aggregate_id,
            event_type: event.event_type,
            event_data: event.event_data,
            processed: Some(event.processed),
            created_at: Some(event.created_at),
        }
    }
}