use std::net::SocketAddr;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use openforce_proto::swarmos::v1::scheduler_server::SchedulerServer;

mod dag;
mod lease_service;
mod worker_spec_builder;
mod retry;
mod quota;
mod kill_switch;
mod client;
mod scheduler_loop;
mod server;

use server::SchedulerService;
use client::GrpcClients;
use scheduler_loop::SchedulerRuntime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "scheduler=debug,info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let session_store_addr = std::env::var("SESSION_STORE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".into());
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50052".into()).parse()?;

    let instance_id = uuid::Uuid::now_v7().to_string();
    tracing::info!("Scheduler instance: {instance_id}");
    tracing::info!("Connecting to Session Store at {session_store_addr}");

    let clients = GrpcClients::connect(&session_store_addr).await?;

    // Spawn the main scheduler loop
    let loop_client = clients.session_store.clone();
    let mut runtime = SchedulerRuntime::new(loop_client);
    tokio::spawn(async move {
        runtime.run().await;
    });

    // Start the gRPC server for external scheduler API
    let service = SchedulerService {
        session_store_addr,
        instance_id: instance_id.clone(),
    };

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<SchedulerServer<SchedulerService>>().await;

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    tracing::info!("Scheduler listening on {grpc_addr}");
    Server::builder()
        .add_service(health_service)
        .add_service(reflection_service)
        .add_service(SchedulerServer::new(service))
        .serve(grpc_addr).await?;

    Ok(())
}
