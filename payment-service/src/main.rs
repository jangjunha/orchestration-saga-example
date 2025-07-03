mod schema;
mod models;
mod handlers;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use diesel::PgConnection;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

use anyhow::Result;
use clap::Parser;
use diesel_async::{pooled_connection::bb8::Pool, AsyncPgConnection};
use diesel::Connection;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::FutureProducer;
use std::time::Duration;
use tokio::time;
use tracing::info;

#[derive(Parser)]
#[command(name = "payment-service")]
struct Args {
    #[arg(long, env = "DATABASE_URL", default_value = "postgres://postgres:password@localhost/payments")]
    database_url: String,
    
    #[arg(long, env = "KAFKA_BROKERS", default_value = "localhost:9092")]
    kafka_brokers: String,
    
    #[arg(long, default_value = "payment-service-commands")]
    command_topic: String,
    
    #[arg(long, default_value = "order-replies")]
    reply_topic: String,
}


#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    // Run migrations first
    info!("Running database migrations...");
    let mut conn = PgConnection::establish(&args.database_url)?;
    conn.run_pending_migrations(MIGRATIONS).map_err(|e| anyhow::anyhow!("Migration error: {}", e))?;
    info!("Migrations completed successfully");

    let config = diesel_async::pooled_connection::AsyncDieselConnectionManager::<AsyncPgConnection>::new(&args.database_url);
    let pool = Pool::builder().build(config).await?;

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &args.kafka_brokers)
        .set("message.timeout.ms", "5000")
        .create()?;

    let consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", "payment-service")
        .set("bootstrap.servers", &args.kafka_brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .create()?;

    consumer.subscribe(&[&args.command_topic])?;

    let command_handler = handlers::CommandHandler::new(pool.clone(), producer.clone(), args.reply_topic.clone());

    tokio::spawn(async move {
        command_handler.run(consumer).await;
    });

    info!("Payment service started");

    loop {
        time::sleep(Duration::from_secs(30)).await;
    }
}