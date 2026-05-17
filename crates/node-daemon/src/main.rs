use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;
use tonic::{Request, Response, Status, transport::Server};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use openforce_proto::swarmos::v1::{
    node_daemon_server::{NodeDaemon, NodeDaemonServer},
    SpawnWorkerRequest, SpawnWorkerResponse,
    GetWorkerStatusRequest, GetWorkerStatusResponse,
    TerminateWorkerRequest, TerminateWorkerResponse,
    HealthCheckRequest, HealthCheckResponse,
};
use openforce_llm_client::LlmClient;

struct WorkerState {
    worker_id: Uuid,
    status: String,
    current_step: String,
    tool_calls_made: u32,
    started_at: chrono::DateTime<chrono::Utc>,
}

struct DaemonService {
    workers: Arc<DashMap<Uuid, WorkerState>>,
    api_key: String,
    base_url: String,
}

#[tonic::async_trait]
impl NodeDaemon for DaemonService {
    async fn spawn_worker(&self, r: Request<SpawnWorkerRequest>) -> Result<Response<SpawnWorkerResponse>, Status> {
        let req = r.into_inner();

        // Verify capability token (supports base64-encoded and raw JSON)
        if !req.capability_token.is_empty() {
            let token: openforce_domain::token::CapabilityToken = {
                let raw = &req.capability_token;
                // Try raw JSON first, then base64
                serde_json::from_str(raw)
                    .or_else(|_| {
                        use base64::Engine;
                        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .decode(raw.as_bytes())
                            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(raw.as_bytes()))
                            .map_err(|e| serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("base64: {e}"))))?;
                        serde_json::from_slice(&bytes)
                    })
                    .map_err(|e| Status::unauthenticated(format!("invalid token: {e}")))?
            };
            if token.is_expired() {
                return Err(Status::unauthenticated("token expired"));
            }
            if let Ok(tid) = Uuid::parse_str(&req.task_id) {
                if token.task_id != tid {
                    return Err(Status::permission_denied("token task mismatch"));
                }
            }
        }

        let worker_id = Uuid::now_v7();
        let state = WorkerState {
            worker_id,
            status: "spawned".into(),
            current_step: "initializing".into(),
            tool_calls_made: 0,
            started_at: chrono::Utc::now(),
        };
        self.workers.insert(worker_id, state);

        let workers = self.workers.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let model = req.model_name.clone();
        let system = req.system_prompt.clone();
        let task_type = req.task_type.clone();
        let max_tokens = req.max_tokens;
        let max_calls = req.max_tool_calls;

        tokio::spawn(async move {
            let client = LlmClient::openai(api_key, base_url, model.clone());
            let prompt = format!(
                "Execute task: {task_type}\nSystem prompt: {system}\nComplete the task and output results."
            );

            // Update status to running
            if let Some(mut s) = workers.get_mut(&worker_id) {
                s.status = "running".into();
                s.current_step = "calling_llm".into();
            }

            // Worker ReAct loop (simplified: single LLM call)
            let mut calls = 0;
            loop {
                if calls >= max_calls as u32 { break; }
                match client.chat(&system, &prompt).await {
                    Ok((text, _)) => {
                        if let Some(mut s) = workers.get_mut(&worker_id) {
                            s.status = "completed".into();
                            s.current_step = format!("llm_response_{calls}");
                            s.tool_calls_made = calls + 1;
                        }
                        tracing::info!("Worker {worker_id} completed: {text:.100}...");
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("Worker {worker_id} LLM call {calls} failed: {e}");
                        calls += 1;
                        if calls >= max_calls as u32 {
                            if let Some(mut s) = workers.get_mut(&worker_id) {
                                s.status = "failed".into();
                                s.current_step = format!("error: {e}");
                            }
                        }
                    }
                }
            }
        });

        Ok(Response::new(SpawnWorkerResponse {
            worker_id: worker_id.to_string(),
            status: "spawned".into(),
        }))
    }

    async fn get_worker_status(&self, r: Request<GetWorkerStatusRequest>) -> Result<Response<GetWorkerStatusResponse>, Status> {
        let wid = Uuid::parse_str(&r.into_inner().worker_id).map_err(|_| Status::invalid_argument("invalid worker_id"))?;
        match self.workers.get(&wid) {
            Some(s) => Ok(Response::new(GetWorkerStatusResponse {
                worker_id: s.worker_id.to_string(),
                status: s.status.clone(),
                current_step: s.current_step.clone(),
                tool_calls_made: s.tool_calls_made as i32,
                elapsed_sec: (chrono::Utc::now() - s.started_at).num_seconds() as i32,
            })),
            None => Err(Status::not_found("worker not found")),
        }
    }

    async fn terminate_worker(&self, r: Request<TerminateWorkerRequest>) -> Result<Response<TerminateWorkerResponse>, Status> {
        let wid = Uuid::parse_str(&r.into_inner().worker_id).map_err(|_| Status::invalid_argument("invalid worker_id"))?;
        let success = self.workers.remove(&wid).is_some();
        Ok(Response::new(TerminateWorkerResponse { success }))
    }

    async fn health_check(&self, _: Request<HealthCheckRequest>) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            active_workers: self.workers.len() as i32,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "node_daemon=debug,info".into()))
        .with(tracing_subscriber::fmt::layer()).init();

    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50060".into()).parse()?;
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".into());

    let instance_id = uuid::Uuid::now_v7().to_string();
    let svc = DaemonService {
        workers: Arc::new(DashMap::new()),
        api_key,
        base_url,
    };

    let (mut hr, health) = tonic_health::server::health_reporter();
    hr.set_serving::<NodeDaemonServer<DaemonService>>().await;

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    let mut server = Server::builder();
    if let Some(bundle) = openforce_mtls::load_bundle_from_env("node-daemon", &instance_id) {
        let cert_pem = String::from_utf8_lossy(&bundle.cert_pem).to_string();
        let key_pem = String::from_utf8_lossy(&bundle.key_pem).to_string();
        let ca_pem = String::from_utf8_lossy(&bundle.ca_cert_pem).to_string();
        let identity = tonic::transport::Identity::from_pem(&cert_pem, &key_pem);
        let client_ca = tonic::transport::Certificate::from_pem(&ca_pem);
        let tls = tonic::transport::server::ServerTlsConfig::new()
            .identity(identity)
            .client_ca_root(client_ca);
        tracing::info!("Node Daemon mTLS enabled");
        server = server.tls_config(tls).map_err(|e| format!("tls: {e}"))?;
    }

    tracing::info!("Node Daemon listening on {grpc_addr}");
    server.add_service(health).add_service(reflection)
        .add_service(NodeDaemonServer::new(svc)).serve(grpc_addr).await?;
    Ok(())
}
