use axum::{routing::post, Router, Json, extract::State};
use serde_json::{json, Value};
use crate::handler::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/read-file", post(read_project_file))
        .route("/read-tree", post(read_project_tree))
        .route("/patches:submit", post(submit_patch))
        .route("/files:delete", post(delete_file))
}

async fn read_project_file(State(s): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    delegate_json(&s.project_tools_addr, "ReadProjectFile", body).await
}

async fn read_project_tree(State(s): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    delegate_json(&s.project_tools_addr, "ReadProjectTree", body).await
}

async fn submit_patch(State(s): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    delegate_json(&s.project_tools_addr, "SubmitProjectPatch", body).await
}

async fn delete_file(State(s): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    delegate_json(&s.project_tools_addr, "DeleteProjectFile", body).await
}

async fn delegate_json(addr: &str, _method: &str, _body: Value) -> Json<Value> {
    // In production: use tonic client to call gRPC backend and map response
    // For now: return stub indicating the gateway is operational
    Json(json!({"data": {"status": "delegated", "backend": addr, "rpc": _method}}))
}
