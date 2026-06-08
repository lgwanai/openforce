use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Service role identity — now a string-based type so that platform operators can
/// register custom service roles beyond the built-in control-plane roles.
/// Built-in roles are provided as factory methods for backward compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceRole(String);

impl ServiceRole {
    // Built-in control-plane roles
    pub fn scheduler() -> Self {
        Self("scheduler".into())
    }
    pub fn node_daemon() -> Self {
        Self("node-daemon".into())
    }
    pub fn worker() -> Self {
        Self("worker".into())
    }
    pub fn effect_gateway() -> Self {
        Self("effect-gateway".into())
    }
    pub fn projection_builder() -> Self {
        Self("projection-builder".into())
    }
    pub fn observer_evolver() -> Self {
        Self("observer-evolver".into())
    }
    pub fn human_approver() -> Self {
        Self("human-approver".into())
    }

    /// Create a custom role from any string
    pub fn custom(name: &str) -> Self {
        Self(name.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(Self(s.to_lowercase()))
    }

    pub fn spiffe_id(&self, instance_id: &str) -> String {
        let trust_domain =
            std::env::var("SPIFFE_TRUST_DOMAIN").unwrap_or_else(|_| "swarmos.internal".into());
        format!("spiffe://{}/{}/{}", trust_domain, self.0, instance_id)
    }
}

impl Default for ServiceRole {
    fn default() -> Self {
        Self::worker()
    }
}

impl std::fmt::Display for ServiceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Extracted from a peer's X.509 certificate after mTLS handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateIdentity {
    pub role: ServiceRole,
    pub instance_id: String,
    pub region: String,
    pub spiffe_id: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
}

/// A complete certificate bundle for one service instance.
#[derive(Debug, Clone)]
pub struct CertificateBundle {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub ca_cert_pem: Vec<u8>,
    pub role: ServiceRole,
    pub instance_id: String,
}
