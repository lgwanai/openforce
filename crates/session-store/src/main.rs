use std::net::SocketAddr;
use sqlx::postgres::PgPoolOptions;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use openforce_proto::swarmos::v1::session_store_server::SessionStoreServer;

mod store;
mod repo;
mod projection;
mod command_handler;
mod server;
use server::SessionStoreService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "session_store=debug,info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://swarmos:swarmos@localhost:5432/swarmos".into());
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into()).parse()?;
    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new().max_connections(20).connect(&database_url).await?;
    tracing::info!("Running migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    let instance_id = uuid::Uuid::now_v7().to_string();
    tracing::info!("Session Store instance: {instance_id}");
    let service = SessionStoreService { pool, instance_id };
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<SessionStoreServer<SessionStoreService>>().await;
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    tracing::info!("Session Store listening on {grpc_addr}");
    Server::builder()
        .add_service(health_service)
        .add_service(reflection_service)
        .add_service(SessionStoreServer::new(service))
        .serve(grpc_addr).await?;
    Ok(())
}
