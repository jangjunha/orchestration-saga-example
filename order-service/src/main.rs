mod schema;
mod models;
mod handlers;
mod outbox;
mod api;

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
use tracing::info;

#[derive(Parser)]
#[command(name = "order-service")]
struct Args {
    #[arg(long, env = "DATABASE_URL", default_value = "postgres://postgres:password@localhost/orders")]
    database_url: String,
    
    #[arg(long, env = "KAFKA_BROKERS", default_value = "localhost:9092")]
    kafka_brokers: String,
    
    #[arg(long, default_value = "order-service-commands")]
    command_topic: String,
    
    #[arg(long, default_value = "order-replies")]
    reply_topic: String,
    
    #[arg(long, env = "PORT", default_value = "3001")]
    port: u16,
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
        .set("group.id", "order-service")
        .set("bootstrap.servers", &args.kafka_brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .create()?;

    let reply_consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", "order-service-replies")
        .set("bootstrap.servers", &args.kafka_brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .create()?;

    consumer.subscribe(&[&args.command_topic])?;
    reply_consumer.subscribe(&[&args.reply_topic])?;

    let outbox_processor = outbox::OutboxProcessor::new(pool.clone(), producer.clone());
    let command_handler = handlers::CommandHandler::new(pool.clone(), producer.clone(), args.reply_topic.clone());
    let saga_manager = handlers::SagaManager::new(pool.clone(), producer.clone());

    tokio::spawn(async move {
        outbox_processor.run().await;
    });

    tokio::spawn(async move {
        command_handler.run(consumer).await;
    });

    tokio::spawn(async move {
        saga_manager.run_reply_handler(reply_consumer).await;
    });

    // Start the web server
    let app_state = api::AppState {
        pool: pool.clone(),
        producer: producer.clone(),
    };
    
    let app = api::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
    
    info!("Order service web server started on port {}", args.port);
    info!("Order service ready to accept HTTP requests at http://0.0.0.0:{}/orders", args.port);

    axum::serve(listener, app).await?;
    
    Ok(())
}