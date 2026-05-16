use std::net::SocketAddr;
use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use openforce_proto::swarmos::v1::effect_gateway_server::EffectGatewayServer;

mod store; mod server; mod outbox;
use store::EffectStore;
use server::EffectGatewayService;
use outbox::OutboxDispatcher;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "effect_gateway=debug,info".into()))
        .with(tracing_subscriber::fmt::layer()).init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://swarmos:swarmos@localhost:5432/swarmos".into());
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50054".into()).parse()?;

    let pool = PgPoolOptions::new().max_connections(10).connect(&database_url).await?;

    // Ensure tables
    for ddl in &[
        "CREATE TABLE IF NOT EXISTS effects (
            effect_id UUID PRIMARY KEY, session_id UUID NOT NULL,
            task_id UUID, tenant_id UUID NOT NULL,
            effect_type VARCHAR(64) NOT NULL DEFAULT 'generic',
            target TEXT NOT NULL DEFAULT '', idempotency_key TEXT NOT NULL UNIQUE,
            payload_ref TEXT NOT NULL DEFAULT '', status VARCHAR(32) NOT NULL DEFAULT 'requested',
            approved_by TEXT, execution_ref TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now())",
        "CREATE TABLE IF NOT EXISTS effect_outbox (
            id BIGSERIAL PRIMARY KEY, effect_id UUID NOT NULL REFERENCES effects(effect_id),
            status VARCHAR(32) NOT NULL DEFAULT 'pending',
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            dispatched_at TIMESTAMPTZ)",
    ] { sqlx::query(ddl).execute(&pool).await?; }

    let store = Arc::new(EffectStore::new(pool.clone()));
    let svc = EffectGatewayService { store };

    // Spawn outbox dispatcher
    let dispatcher = OutboxDispatcher::new(pool.clone());
    tokio::spawn(async move { dispatcher.run().await; });

    let (mut hr, health) = tonic_health::server::health_reporter();
    hr.set_serving::<EffectGatewayServer<EffectGatewayService>>().await;

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET).build_v1()?;

    tracing::info!("Effect Gateway on {grpc_addr}");
    Server::builder().add_service(health).add_service(reflection)
        .add_service(EffectGatewayServer::new(svc)).serve(grpc_addr).await?;
    Ok(())
}
