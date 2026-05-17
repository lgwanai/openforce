use openforce_domain::identity::{CertificateIdentity, ServiceRole};
use crate::ca::CertificateAuthority;
use crate::error::{MTLSError, MTLSResult};

/// Verifies peer certificates from mTLS handshakes with optional role allowlisting.
pub struct CertificateVerifier {
    ca: CertificateAuthority,
    allowed_roles: Vec<ServiceRole>,
}

impl CertificateVerifier {
    pub fn new(ca: CertificateAuthority) -> Self {
        Self { ca, allowed_roles: vec![] }
    }

    pub fn with_allowed_roles(mut self, roles: Vec<ServiceRole>) -> Self {
        self.allowed_roles = roles;
        self
    }

    pub fn verify_peer(&self, cert_der: &[u8]) -> MTLSResult<CertificateIdentity> {
        let identity = self.ca.verify(cert_der)?;
        if !self.allowed_roles.is_empty() && !self.allowed_roles.contains(&identity.role) {
            return Err(MTLSError::RoleMismatch {
                expected: format!("one of {:?}", self.allowed_roles),
                actual: format!("{:?}", identity.role),
            });
        }
        Ok(identity)
    }

    pub fn ca_cert_pem(&self) -> &[u8] { self.ca.ca_cert_pem() }
}
