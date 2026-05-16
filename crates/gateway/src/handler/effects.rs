use axum::{routing::{get, post}, Router, Json, extract::{State, Path}};
use serde_json::{json, Value};
use crate::handler::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/requests", post(request_effect))
        .route("/{id}", get(get_effect))
        .route("/{id}:approve", post(approve_effect))
}

async fn request_effect(State(_s): State<AppState>, Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
async fn get_effect(State(_s): State<AppState>, Path(_id): Path<String>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
async fn approve_effect(State(_s): State<AppState>, Path(_id): Path<String>, Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
