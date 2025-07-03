use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::inventory)]
pub struct Inventory {
    pub id: Uuid,
    pub product_id: Uuid,
    pub available_quantity: i32,
    pub reserved_quantity: i32,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::reservations)]
pub struct Reservation {
    pub id: Uuid,
    pub product_id: Uuid,
    pub order_id: Uuid,
    pub quantity: i32,
    pub status: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = crate::schema::reservations)]
pub struct NewReservation {
    pub id: Uuid,
    pub product_id: Uuid,
    pub order_id: Uuid,
    pub quantity: i32,
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