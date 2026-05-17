use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::ca::CertificateAuthority;
use crate::error::MTLSResult;
use openforce_domain::identity::CertificateBundle;

/// Zero-downtime certificate rotation for a service instance.
pub struct RotatingCertificateManager {
    ca: Arc<CertificateAuthority>,
    instance_id: String,
    current: RwLock<CertificateBundle>,
}

impl RotatingCertificateManager {
    pub fn new(ca: Arc<CertificateAuthority>, current: CertificateBundle) -> Self {
        let instance_id = current.instance_id.clone();
        Self { ca, instance_id, current: RwLock::new(current) }
    }

    pub async fn current_bundle(&self) -> CertificateBundle {
        self.current.read().await.clone()
    }

    pub async fn rotate(&self) -> MTLSResult<CertificateBundle> {
        let role = self.current.read().await.role;
        let new_bundle = self.ca.issue(role, &self.instance_id, "")?;
        *self.current.write().await = new_bundle.clone();
        info!("certificate rotated for {:?}/{}", role, self.instance_id);
        Ok(new_bundle)
    }
}
