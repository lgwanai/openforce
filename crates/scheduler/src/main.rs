use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use openforce_proto::swarmos::v1::scheduler_server::SchedulerServer;

mod dag; mod lease_service; mod worker_spec_builder; mod retry; mod quota;
mod kill_switch; mod client; mod scheduler_loop; mod server;
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

    let service = SchedulerService { session_store_addr, instance_id };

    let (mut hr, health) = tonic_health::server::health_reporter();
    hr.set_serving::<SchedulerServer<SchedulerService>>().await;

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET).build_v1()?;

    tracing::info!("Scheduler listening on {grpc_addr}");
    Server::builder().add_service(health).add_service(reflection)
        .add_service(SchedulerServer::new(service)).serve(grpc_addr).await?;
    Ok(())
}
