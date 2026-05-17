use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Every control-plane entity gets an independent identity (architecture doc 22.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceRole {
    Scheduler,
    NodeDaemon,
    Worker,
    EffectGateway,
    ProjectionBuilder,
    ObserverEvolver,
    HumanApprover,
}

impl ServiceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduler => "scheduler",
            Self::NodeDaemon => "node-daemon",
            Self::Worker => "worker",
            Self::EffectGateway => "effect-gateway",
            Self::ProjectionBuilder => "projection-builder",
            Self::ObserverEvolver => "observer-evolver",
            Self::HumanApprover => "human-approver",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "scheduler" => Some(Self::Scheduler),
            "node-daemon" => Some(Self::NodeDaemon),
            "worker" => Some(Self::Worker),
            "effect-gateway" => Some(Self::EffectGateway),
            "projection-builder" => Some(Self::ProjectionBuilder),
            "observer-evolver" => Some(Self::ObserverEvolver),
            "human-approver" => Some(Self::HumanApprover),
            _ => None,
        }
    }

    pub fn spiffe_id(&self, instance_id: &str) -> String {
        format!("spiffe://swarmos.internal/{}/{}", self.as_str(), instance_id)
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
