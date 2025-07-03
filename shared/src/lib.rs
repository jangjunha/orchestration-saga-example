use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: Uuid,
    pub saga_id: Uuid,
    pub command_type: CommandType,
    pub payload: serde_json::Value,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandType {
    CreateOrder,
    ProcessPayment,
    ReserveInventory,
    ApproveOrder,
    CompensatePayment,
    CompensateInventory,
    CancelOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandReply {
    pub id: Uuid,
    pub command_id: Uuid,
    pub saga_id: Uuid,
    pub status: CommandStatus,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandStatus {
    Success,
    Failed,
    Compensated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStep {
    pub command_type: CommandType,
    pub compensation_type: Option<CommandType>,
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaTransaction {
    pub id: Uuid,
    pub steps: Vec<SagaStep>,
    pub current_step: usize,
    pub status: SagaStatus,
    pub context: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SagaStatus {
    Started,
    InProgress,
    Completed,
    Compensating,
    Compensated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderData {
    pub order_id: Uuid,
    pub customer_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub total_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentData {
    pub order_id: Uuid,
    pub amount: f64,
    pub payment_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryData {
    pub product_id: Uuid,
    pub quantity: i32,
    pub order_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub processed: bool,
    pub created_at: DateTime<Utc>,
}

impl SagaTransaction {
    pub fn new(order_data: OrderData) -> Self {
        let steps = vec![
            SagaStep {
                command_type: CommandType::CreateOrder,
                compensation_type: Some(CommandType::CancelOrder),
                service_name: "order-service".to_string(),
            },
            SagaStep {
                command_type: CommandType::ProcessPayment,
                compensation_type: Some(CommandType::CompensatePayment),
                service_name: "payment-service".to_string(),
            },
            SagaStep {
                command_type: CommandType::ReserveInventory,
                compensation_type: Some(CommandType::CompensateInventory),
                service_name: "inventory-service".to_string(),
            },
            SagaStep {
                command_type: CommandType::ApproveOrder,
                compensation_type: None,
                service_name: "order-service".to_string(),
            },
        ];

        let mut context = HashMap::new();
        context.insert("order_data".to_string(), serde_json::to_value(order_data).unwrap());

        Self {
            id: Uuid::new_v4(),
            steps,
            current_step: 0,
            status: SagaStatus::Started,
            context,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn next_step(&mut self) -> Option<&SagaStep> {
        if self.current_step < self.steps.len() {
            Some(&self.steps[self.current_step])
        } else {
            None
        }
    }

    pub fn advance_step(&mut self) {
        if self.current_step < self.steps.len() {
            self.current_step += 1;
            self.updated_at = Utc::now();
        }
    }

    pub fn get_compensation_steps(&self) -> Vec<&SagaStep> {
        self.steps[0..self.current_step]
            .iter()
            .rev()
            .filter(|step| step.compensation_type.is_some())
            .collect()
    }
}

impl Command {
    pub fn new(saga_id: Uuid, command_type: CommandType, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            saga_id,
            command_type,
            payload,
            idempotency_key: format!("{}_{}", saga_id, Uuid::new_v4()),
            created_at: Utc::now(),
        }
    }
}

impl CommandReply {
    pub fn success(command_id: Uuid, saga_id: Uuid, result: Option<serde_json::Value>) -> Self {
        Self {
            id: Uuid::new_v4(),
            command_id,
            saga_id,
            status: CommandStatus::Success,
            result,
            error: None,
            created_at: Utc::now(),
        }
    }

    pub fn failed(command_id: Uuid, saga_id: Uuid, error: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            command_id,
            saga_id,
            status: CommandStatus::Failed,
            result: None,
            error: Some(error),
            created_at: Utc::now(),
        }
    }
}