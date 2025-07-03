use anyhow::Result;
use diesel::prelude::*;
use diesel_async::{pooled_connection::bb8::Pool, AsyncPgConnection, RunQueryDsl};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;
use tokio::time;
use tracing::{error, info};
use crate::models::*;
use crate::schema::*;

type DbPool = Pool<AsyncPgConnection>;

pub struct OutboxProcessor {
    pool: DbPool,
    producer: FutureProducer,
}

impl OutboxProcessor {
    pub fn new(pool: DbPool, producer: FutureProducer) -> Self {
        Self { pool, producer }
    }

    pub async fn run(&self) {
        let mut interval = time::interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.process_outbox_events().await {
                error!("Error processing outbox events: {}", e);
            }
        }
    }

    async fn process_outbox_events(&self) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let unprocessed_events = outbox_events::table
            .filter(outbox_events::processed.eq(false))
            .order(outbox_events::created_at.asc())
            .limit(100)
            .load::<DbOutboxEvent>(&mut conn)
            .await?;

        for event in unprocessed_events {
            if let Err(e) = self.publish_event(&event).await {
                error!("Failed to publish event {}: {}", event.id, e);
                continue;
            }

            diesel::update(outbox_events::table.filter(outbox_events::id.eq(event.id)))
                .set(outbox_events::processed.eq(true))
                .execute(&mut conn)
                .await?;

            info!("Published outbox event: {}", event.id);
        }

        Ok(())
    }

    async fn publish_event(&self, event: &DbOutboxEvent) -> Result<()> {
        let topic = match event.event_type.as_str() {
            "OrderCreated" => "order-events",
            "PaymentProcessed" => "payment-events",
            "InventoryReserved" => "inventory-events",
            _ => "domain-events",
        };

        let json = serde_json::to_string(&event.event_data)?;
        let key = event.aggregate_id.to_string();
        let record = FutureRecord::to(topic)
            .payload(&json)
            .key(&key);

        self.producer.send(record, Duration::from_secs(5)).await
            .map_err(|(e, _)| anyhow::anyhow!("Failed to publish event: {}", e))?;

        Ok(())
    }
}