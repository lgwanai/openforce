pub mod project_tools; pub mod approvals; pub mod effects;
use axum::{response::Json, extract::State};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct AppState {
    pub session_store_addr: String,
    pub project_tools_addr: String,
    pub effect_gateway_addr: String,
}

pub async fn health() -> Json<Value> { Json(json!({"status": "ok"})) }
