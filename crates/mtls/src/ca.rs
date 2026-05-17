use std::sync::Arc;
use dashmap::DashMap;
use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, PKCS_ED25519};
use ring::rand::SystemRandom;
use time::{Duration, OffsetDateTime};
use tracing::{info, warn};

use crate::error::{MTLSError, MTLSResult};
use openforce_domain::identity::{CertificateBundle, CertificateIdentity, ServiceRole};

#[derive(Debug, Clone)]
struct IssuedCertificate {
    serial: String,
    role: ServiceRole,
    instance_id: String,
    not_after: OffsetDateTime,
}

pub struct CertificateAuthority {
    ca_key: KeyPair,
    ca_cert: Certificate,
    ca_cert_pem: Vec<u8>,
    issued: Arc<DashMap<String, IssuedCertificate>>,
    revoked: Arc<DashMap<String, String>>,
    leaf_ttl_days: i64,
}

impl CertificateAuthority {
    pub fn new(organization: &str, leaf_ttl_days: i64) -> MTLSResult<Self> {
        let mut ca_params = CertificateParams::new(vec![organization.into()])
            .map_err(|e| MTLSError::CaError { detail: e.to_string() })?;
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.distinguished_name = {
            let mut dn = DistinguishedName::new();
            dn.push(DnType::OrganizationName, organization);
            dn.push(DnType::CommonName, format!("{organization} Root CA"));
            dn
        };
        let ca_key = KeyPair::generate_for(&PKCS_ED25519)
            .map_err(|e| MTLSError::CaError { detail: e.to_string() })?;
        let ca_cert = ca_params.self_signed(&ca_key)
            .map_err(|e| MTLSError::CaError { detail: e.to_string() })?;
        let ca_cert_pem = ca_cert.pem().into_bytes();
        info!("CA initialized: org={organization}, leaf_ttl={leaf_ttl_days}d");
        Ok(Self {
            ca_key, ca_cert, ca_cert_pem,
            issued: Arc::new(DashMap::new()), revoked: Arc::new(DashMap::new()),
            leaf_ttl_days,
        })
    }

    pub fn issue(&self, role: ServiceRole, instance_id: &str, _region: &str) -> MTLSResult<CertificateBundle> {
        let spiffe_id = role.spiffe_id(instance_id);
        let serial = Self::gen_serial();
        let mut params = CertificateParams::new(vec![spiffe_id.clone()])
            .map_err(|e| MTLSError::CaError { detail: e.to_string() })?;
        params.distinguished_name = {
            let mut dn = DistinguishedName::new();
            dn.push(DnType::CommonName, &spiffe_id);
            dn.push(DnType::OrganizationName, "swarmos.internal");
            dn
        };
        let now = OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + Duration::days(self.leaf_ttl_days);

        let leaf_key = KeyPair::generate_for(&PKCS_ED25519)
            .map_err(|e| MTLSError::CaError { detail: e.to_string() })?;
        let not_after = params.not_after;
        let leaf_cert = params.signed_by(&leaf_key, &self.ca_cert, &self.ca_key)
            .map_err(|e| MTLSError::CaError { detail: e.to_string() })?;

        self.issued.insert(serial.clone(), IssuedCertificate {
            serial: serial.clone(), role, instance_id: instance_id.into(),
            not_after,
        });
        info!("cert issued: role={role:?} instance={instance_id} serial={serial}");
        Ok(CertificateBundle {
            cert_pem: leaf_cert.pem().into_bytes(),
            key_pem: leaf_key.serialize_pem().into_bytes(),
            ca_cert_pem: self.ca_cert_pem.clone(),
            role, instance_id: instance_id.into(),
        })
    }

    pub fn verify(&self, peer_cert_der: &[u8]) -> MTLSResult<CertificateIdentity> {
        let (_, cert) = x509_parser::parse_x509_certificate(peer_cert_der)
            .map_err(|e| MTLSError::VerificationFailed { detail: e.to_string() })?;

        let spiffe_id = cert.subject_alternative_name()
            .map_err(|e| MTLSError::IdentityExtractionFailed { detail: e.to_string() })?
            .and_then(|san| {
                san.value.general_names.iter().find_map(|n| {
                    match n {
                        x509_parser::extensions::GeneralName::DNSName(s) => {
                            if s.starts_with("spiffe://") { Some(s.to_string()) } else { None }
                        }
                        x509_parser::extensions::GeneralName::URI(s) => {
                            if s.starts_with("spiffe://") { Some(s.to_string()) } else { None }
                        }
                        _ => None,
                    }
                })
            })
            .ok_or_else(|| MTLSError::VerificationFailed { detail: "no SPIFFE SAN".into() })?;

        let path = spiffe_id.strip_prefix("spiffe://swarmos.internal/").unwrap_or(&spiffe_id);
        let parts: Vec<&str> = path.split('/').collect();
        let role = parts.first().and_then(|r| ServiceRole::from_str(r))
            .ok_or_else(|| MTLSError::RoleMismatch {
                expected: "valid-role".into(),
                actual: parts.first().map(|s| s.to_string()).unwrap_or_default(),
            })?;
        let instance_id = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

        let serial = cert.raw_serial_as_string();
        if self.revoked.contains_key(&serial) {
            return Err(MTLSError::CertificateRevoked { serial });
        }
        let nb = cert.validity().not_before.to_datetime();
        let na = cert.validity().not_after.to_datetime();
        Ok(CertificateIdentity {
            role, instance_id, region: String::new(), spiffe_id,
            not_before: chrono::DateTime::from_timestamp(nb.unix_timestamp(), 0).unwrap_or_default(),
            not_after: chrono::DateTime::from_timestamp(na.unix_timestamp(), 0).unwrap_or_default(),
        })
    }

    pub fn revoke(&self, serial: &str, reason: &str) {
        self.revoked.insert(serial.into(), reason.into());
        self.issued.remove(serial);
        warn!("cert revoked: serial={serial} reason={reason}");
    }

    pub fn is_revoked(&self, serial: &str) -> bool { self.revoked.contains_key(serial) }
    pub fn ca_cert_pem(&self) -> &[u8] { &self.ca_cert_pem }

    fn gen_serial() -> String {
        let rand = SystemRandom::new();
        let mut bytes = [0u8; 16];
        ring::rand::SecureRandom::fill(&rand, &mut bytes).expect("CRNG must not fail");
        hex::encode(bytes)
    }
}
