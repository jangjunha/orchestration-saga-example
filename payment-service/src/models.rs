use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Queryable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::payments)]
pub struct Payment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub amount: bigdecimal::BigDecimal,
    pub payment_method: String,
    pub status: String,
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Insertable, Serialize)]
#[diesel(table_name = crate::schema::payments)]
pub struct NewPayment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub amount: bigdecimal::BigDecimal,
    pub payment_method: String,
    pub status: String,
}

#[derive(Debug, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::processed_commands)]
pub struct ProcessedCommand {
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub result: Option<serde_json::Value>,
    pub processed_at: Option<DateTime<Utc>>,
}