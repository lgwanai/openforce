use std::net::SocketAddr;
use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use openforce_proto::swarmos::v1::{
    project_tool_service_server::ProjectToolServiceServer,
    approval_service_server::ApprovalServiceServer,
};

mod server;
mod approval_store;

use server::{ProjectToolServiceImpl, ApprovalServiceImpl};
use approval_store::ApprovalStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "project_tools=debug,info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://swarmos:swarmos@localhost:5432/swarmos".into());
    let grpc_addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50053".into()).parse()?;

    let pool = PgPoolOptions::new().max_connections(20).connect(&database_url).await?;

    // Run migrations for approval tables
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS approval_requests (
            approval_request_id UUID PRIMARY KEY, session_id UUID NOT NULL,
            task_id UUID NOT NULL, task_attempt INTEGER NOT NULL,
            lease_id UUID NOT NULL, fencing_token BIGINT NOT NULL,
            worker_spec_id UUID NOT NULL, tool_name VARCHAR(64) NOT NULL,
            patch_risk_level VARCHAR(16) NOT NULL, target_paths JSONB NOT NULL DEFAULT '[]',
            base_snapshot_id TEXT NOT NULL, payload_sha256 TEXT NOT NULL,
            status VARCHAR(16) NOT NULL DEFAULT 'pending_human',
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            expires_at TIMESTAMPTZ NOT NULL,
            resolved_at TIMESTAMPTZ, approved_by TEXT, rejected_by TEXT, rejected_reason TEXT
        )"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS approval_tokens (
            approval_token_id UUID PRIMARY KEY, approval_request_id UUID NOT NULL,
            session_id UUID NOT NULL, task_id UUID NOT NULL,
            task_attempt INTEGER NOT NULL, lease_id UUID NOT NULL,
            fencing_token BIGINT NOT NULL, worker_spec_id UUID NOT NULL,
            tool_name VARCHAR(64) NOT NULL, target_paths JSONB NOT NULL DEFAULT '[]',
            base_snapshot_id TEXT NOT NULL, payload_sha256 TEXT NOT NULL,
            status VARCHAR(16) NOT NULL DEFAULT 'approved',
            usage_limit INTEGER NOT NULL DEFAULT 1, usage_count INTEGER NOT NULL DEFAULT 0,
            issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            expires_at TIMESTAMPTZ NOT NULL,
            approved_by TEXT NOT NULL, signature BYTEA NOT NULL
        )"
    ).execute(&pool).await?;

    let approval_store = Arc::new(ApprovalStore::new(pool.clone()));
    let tool_service = ProjectToolServiceImpl { pool };
    let approval_service = ApprovalServiceImpl { approval_store };

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<ProjectToolServiceServer<ProjectToolServiceImpl>>().await;

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    tracing::info!("Project Tools listening on {grpc_addr}");
    Server::builder()
        .add_service(health_service)
        .add_service(reflection_service)
        .add_service(ProjectToolServiceServer::new(tool_service))
        .add_service(ApprovalServiceServer::new(approval_service))
        .serve(grpc_addr).await?;

    Ok(())
}
