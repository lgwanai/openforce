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
    pub token_verifier: Option<middleware::TokenVerifier>,
}

impl App {
    pub fn new(addr: SocketAddr) -> Self {
        let token_verifier = Self::create_verifier_from_env();
        Self {
            addr,
            session_store_addr: std::env::var("SESSION_STORE_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".into()),
            project_tools_addr: std::env::var("PROJECT_TOOLS_ADDR").unwrap_or_else(|_| "127.0.0.1:50053".into()),
            effect_gateway_addr: std::env::var("EFFECT_GATEWAY_ADDR").unwrap_or_else(|_| "127.0.0.1:50054".into()),
            token_verifier,
        }
    }

    /// Load the Scheduler's Ed25519 public key for capability token verification.
    /// REQUIRED in production: set SCHEDULER_PUBLIC_KEY_PATH to a file containing 32-byte Ed25519 public key.
    /// If not set, a warning is logged and signature verification is skipped (INSECURE — dev/test only).
    fn create_verifier_from_env() -> Option<middleware::TokenVerifier> {
        match std::env::var("SCHEDULER_PUBLIC_KEY_PATH") {
            Ok(path) => {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        if bytes.len() != 32 {
                            tracing::error!("SCHEDULER_PUBLIC_KEY_PATH={path}: expected 32 bytes, got {}", bytes.len());
                            None
                        } else {
                            tracing::info!("scheduler public key loaded from {path}");
                            Some(middleware::TokenVerifier::from_public_key_bytes(bytes))
                        }
                    }
                    Err(e) => {
                        tracing::error!("failed to read SCHEDULER_PUBLIC_KEY_PATH={path}: {e}");
                        None
                    }
                }
            }
            Err(_) => {
                tracing::warn!("SCHEDULER_PUBLIC_KEY_PATH not set — token signature verification DISABLED (INSECURE, dev/test only)");
                None
            }
        }
    }

    pub fn with_token_verifier(mut self, verifier: middleware::TokenVerifier) -> Self {
        self.token_verifier = Some(verifier);
        self
    }

    pub fn router(&self) -> Router {
        let state = handler::AppState {
            session_store_addr: self.session_store_addr.clone(),
            project_tools_addr: self.project_tools_addr.clone(),
            effect_gateway_addr: self.effect_gateway_addr.clone(),
        };

        let mut router = Router::new()
            .nest("/api/v1/project-tools",
                handler::project_tools::routes()
                    .layer(axum::middleware::from_fn(middleware::require_capability_token)))
            .nest("/api/v1/approvals",
                handler::approvals::routes()
                    .layer(axum::middleware::from_fn(middleware::require_capability_token)))
            .nest("/api/v1/effects",
                handler::effects::routes()
                    .layer(axum::middleware::from_fn(middleware::require_capability_token)))
            .route("/health", axum::routing::get(handler::health))
            .layer(ServiceBuilder::new()
                .layer(CorsLayer::new()
                    .allow_origin(std::env::var("CORS_ALLOWED_ORIGINS")
                        .unwrap_or_else(|_| "http://localhost:3000".into())
                        .split(',')
                        .filter_map(|o| o.trim().parse().ok())
                        .collect::<Vec<_>>())
                    .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                    .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]))
                .layer(middleware::LoggingLayer)
                .layer(middleware::RequestIdLayer))
            .with_state(state);

        // Inject TokenVerifier into router extensions so middleware can access it
        if let Some(verifier) = &self.token_verifier {
            router = router.layer(axum::Extension(verifier.clone()));
        }

        router
    }
}
