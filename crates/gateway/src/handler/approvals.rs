use axum::{routing::{get, post}, Router, Json, extract::{State, Path}};
use serde_json::{json, Value};
use crate::handler::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(":request", post(create_approval))
        .route("/{id}", get(get_approval))
        .route("/{id}:approve", post(approve_approval))
        .route("/{id}:reject", post(reject_approval))
        .route("/tokens:consume", post(consume_token))
}

async fn create_approval(State(_s): State<AppState>, Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
async fn get_approval(State(_s): State<AppState>, Path(_id): Path<String>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
async fn approve_approval(State(_s): State<AppState>, Path(_id): Path<String>, Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
async fn reject_approval(State(_s): State<AppState>, Path(_id): Path<String>, Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
async fn consume_token(State(_s): State<AppState>, Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({"data": {"status": "delegated"}}))
}
