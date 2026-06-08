pub mod approvals;
pub mod effects;
pub mod project_tools;
use axum::response::Json;
use serde_json::{json, Value};

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    pub session_store_addr: String,
    pub project_tools_addr: String,
    pub effect_gateway_addr: String,
}

pub async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}
