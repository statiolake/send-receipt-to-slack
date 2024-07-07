use anyhow::Result;
use aws_config::BehaviorVersion;
use axum::{extract::State, routing::post, Json, Router};
use base64::{engine::general_purpose, Engine as _};
use lambda_http::{run, Error};
use receipt_analyzer::{create_bedrock_client, Receipt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    bedrock_client: Arc<aws_sdk_bedrockruntime::Client>,
}

#[derive(Deserialize)]
struct ReceiptRequest {
    image: String,
}

#[derive(Serialize)]
struct AnalysisResponse {
    result: Receipt,
}

#[axum::debug_handler]
async fn analyze_receipt(
    State(state): State<AppState>,
    Json(payload): Json<ReceiptRequest>,
) -> Result<Json<Value>, axum::http::StatusCode> {
    let image_data = general_purpose::STANDARD.decode(&payload.image).unwrap();
    match receipt_analyzer::analyze(&state.bedrock_client, &image_data).await {
        Ok(result) => Ok(Json(serde_json::json!(AnalysisResponse { result }))),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let bedrock_client = Arc::new(create_bedrock_client().await);

    let app_state = AppState { bedrock_client };

    let app = Router::new()
        .route("/analyze", post(analyze_receipt))
        .with_state(app_state);

    run(app).await
}
