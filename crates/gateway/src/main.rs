use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use axum::serve;
use tokio::net::TcpListener;

mod middleware;
mod handler;
mod mapper;
mod bootstrap;

use bootstrap::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "gateway=debug,info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr: SocketAddr = std::env::var("HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into()).parse()?;

    let app = App::new(addr);
    let router = app.router();

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("REST Gateway listening on {addr}");
    serve(listener, router).await?;

    Ok(())
}
