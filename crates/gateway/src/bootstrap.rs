use std::net::SocketAddr;
use axum::Router;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use crate::middleware;
use crate::handler;

pub struct App {
    pub addr: SocketAddr,
    pub session_store_addr: String,
    pub project_tools_addr: String,
    pub effect_gateway_addr: String,
}

impl App {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            session_store_addr: "127.0.0.1:50051".into(),
            project_tools_addr: "127.0.0.1:50053".into(),
            effect_gateway_addr: "127.0.0.1:50054".into(),
        }
    }

    pub fn router(&self) -> Router {
        let state = handler::AppState {
            session_store_addr: self.session_store_addr.clone(),
            project_tools_addr: self.project_tools_addr.clone(),
            effect_gateway_addr: self.effect_gateway_addr.clone(),
        };

        Router::new()
            .nest("/api/v1/project-tools", handler::project_tools::routes())
            .nest("/api/v1/approvals", handler::approvals::routes())
            .nest("/api/v1/effects", handler::effects::routes())
            .route("/health", axum::routing::get(handler::health))
            .layer(ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(middleware::LoggingLayer)
                .layer(middleware::RequestIdLayer))
            .with_state(state)
    }
}
