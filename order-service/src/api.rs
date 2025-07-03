use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use diesel_async::{pooled_connection::bb8::Pool, AsyncPgConnection};
use rdkafka::producer::FutureProducer;
use serde::{Deserialize, Serialize};
use shared::*;
use uuid::Uuid;
use crate::handlers::SagaManager;

type DbPool = Pool<AsyncPgConnection>;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub producer: FutureProducer,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub customer_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub total_amount: f64,
}

#[derive(Debug, Serialize)]
pub struct CreateOrderResponse {
    pub order_id: Uuid,
    pub saga_id: Uuid,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/orders", post(create_order))
        .route("/health", axum::routing::get(health_check))
        .with_state(state)
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
}

pub async fn create_order(
    State(state): State<AppState>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, (StatusCode, Json<ErrorResponse>)> {
    let order_id = Uuid::new_v4();
    
    let order_data = OrderData {
        order_id,
        customer_id: request.customer_id,
        product_id: request.product_id,
        quantity: request.quantity,
        total_amount: request.total_amount,
    };

    let saga = SagaTransaction::new(order_data);
    let saga_id = saga.id;

    let saga_manager = SagaManager::new(state.pool, state.producer);
    
    match saga_manager.start_saga(saga).await {
        Ok(_) => {
            tracing::info!("Started saga {} for order {}", saga_id, order_id);
            Ok(Json(CreateOrderResponse {
                order_id,
                saga_id,
                status: "started".to_string(),
                message: "Order saga transaction has been initiated".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to start saga: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to start order saga: {}", e),
                }),
            ))
        }
    }
}

pub async fn health_check() -> &'static str {
    "OK"
}