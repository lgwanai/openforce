use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use openforce_proto::swarmos::v1::scheduler_server::SchedulerServer;

mod dag; mod lease_service; mod worker_spec_builder; mod retry; mod quota;
mod kill_switch; mod client; mod scheduler_loop; mod server; mod capability_token;
use server::SchedulerService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "scheduler=debug,info".into()))
        .with(tracing_subscriber::fmt::layer()).init();

    let session_store_addr = std::env::var("SESSION_STORE_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".into());
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR").unwrap_or_else(|_| "0.0.0.0:50052".into()).parse()?;
    let instance_id = uuid::Uuid::now_v7().to_string();
    tracing::info!("Scheduler {instance_id} connecting to SessionStore at {session_store_addr}");

    let token_issuer = {
        let key = std::env::var("SCHEDULER_SIGNING_KEY")
            .ok()
            .and_then(|b64| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).ok()
            });
        key.map(|k| Arc::new(capability_token::CapabilityTokenIssuer::new(&k)
            .expect("invalid SCHEDULER_SIGNING_KEY")))
    };

    let service = SchedulerService { session_store_addr, instance_id: instance_id.clone(), token_issuer };

    let (mut hr, health) = tonic_health::server::health_reporter();
    hr.set_serving::<SchedulerServer<SchedulerService>>().await;

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET).build_v1()?;

    let mut server = Server::builder();

    if let Some(bundle) = openforce_mtls::load_bundle_from_env("scheduler", &instance_id) {
        let cert_pem = String::from_utf8_lossy(&bundle.cert_pem).to_string();
        let key_pem = String::from_utf8_lossy(&bundle.key_pem).to_string();
        let ca_pem = String::from_utf8_lossy(&bundle.ca_cert_pem).to_string();
        let identity = tonic::transport::Identity::from_pem(&cert_pem, &key_pem);
        let client_ca = tonic::transport::Certificate::from_pem(&ca_pem);
        let tls = tonic::transport::server::ServerTlsConfig::new()
            .identity(identity)
            .client_ca_root(client_ca);
        tracing::info!("Scheduler mTLS enabled");
        server = server.tls_config(tls).map_err(|e| format!("tls: {e}"))?;
    }

    tracing::info!("Scheduler listening on {grpc_addr}");
    server.add_service(health).add_service(reflection)
        .add_service(SchedulerServer::new(service)).serve(grpc_addr).await?;
    Ok(())
}
