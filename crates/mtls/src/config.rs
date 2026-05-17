use std::sync::Arc;
use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};

use crate::error::{MTLSError, MTLSResult};
use openforce_domain::identity::CertificateBundle;

fn load_cert_chain(pem: &[u8]) -> MTLSResult<Vec<CertificateDer<'static>>> {
    let mut reader = std::io::BufReader::new(pem);
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MTLSError::TlsConfigError { detail: e.to_string() })
}

fn load_private_key(pem: &[u8]) -> MTLSResult<PrivateKeyDer<'static>> {
    let mut reader = std::io::BufReader::new(pem);
    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| MTLSError::TlsConfigError { detail: e.to_string() })?
        .ok_or_else(|| MTLSError::TlsConfigError { detail: "no private key found".into() })?;
    Ok(key)
}

fn load_ca_store(pem: &[u8]) -> MTLSResult<RootCertStore> {
    let mut store = RootCertStore::empty();
    let ca_certs = load_cert_chain(pem)?;
    for ca in ca_certs {
        store.add(ca).map_err(|e| MTLSError::TlsConfigError { detail: e.to_string() })?;
    }
    Ok(store)
}

/// Load certificate bundle from environment variables or file paths.
pub fn load_bundle_from_env(role: &str, instance_id: &str) -> Option<CertificateBundle> {
    let cert_pem = std::env::var("SWARMOS_TLS_CERT").ok()
        .or_else(|| std::fs::read_to_string(
            format!("{}/{}/cert.pem", std::env::var("SWARMOS_TLS_DIR").unwrap_or_default(), role)
        ).ok())?;
    let key_pem = std::env::var("SWARMOS_TLS_KEY").ok()
        .or_else(|| std::fs::read_to_string(
            format!("{}/{}/key.pem", std::env::var("SWARMOS_TLS_DIR").unwrap_or_default(), role)
        ).ok())?;
    let ca_cert_pem = std::env::var("SWARMOS_CA_CERT").ok()
        .or_else(|| std::fs::read_to_string(
            format!("{}/ca.pem", std::env::var("SWARMOS_TLS_DIR").unwrap_or_default())
        ).ok())?;
    let role = openforce_domain::identity::ServiceRole::from_str(role)?;
    Some(CertificateBundle { cert_pem: cert_pem.into_bytes(), key_pem: key_pem.into_bytes(), ca_cert_pem: ca_cert_pem.into_bytes(), role, instance_id: instance_id.into() })
}

pub fn build_server_rustls_config(bundle: &CertificateBundle) -> MTLSResult<rustls::ServerConfig> {
    TlsConfigBuilder::server_config_with_mutual_tls(bundle)
}

pub struct TlsConfigBuilder;

impl TlsConfigBuilder {
    pub fn server_config_with_mutual_tls(bundle: &CertificateBundle) -> MTLSResult<ServerConfig> {
        let _ = default_provider().install_default()
            .map_err(|_| MTLSError::TlsConfigError { detail: "crypto provider init failed".into() })?;

        let cert_chain = load_cert_chain(&bundle.cert_pem)?;
        let key = load_private_key(&bundle.key_pem)?;
        let client_ca = load_ca_store(&bundle.ca_cert_pem)?;

        let verifier = WebPkiClientVerifier::builder(Arc::new(client_ca))
            .build()
            .map_err(|e| MTLSError::TlsConfigError { detail: e.to_string() })?;

        ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(cert_chain, key)
            .map_err(|e| MTLSError::TlsConfigError { detail: e.to_string() })
    }

    pub fn client_config_with_mutual_tls(bundle: &CertificateBundle) -> MTLSResult<ClientConfig> {
        let _ = default_provider().install_default()
            .map_err(|_| MTLSError::TlsConfigError { detail: "crypto provider init failed".into() })?;

        let cert_chain = load_cert_chain(&bundle.cert_pem)?;
        let key = load_private_key(&bundle.key_pem)?;
        let server_ca = load_ca_store(&bundle.ca_cert_pem)?;

        ClientConfig::builder()
            .with_root_certificates(server_ca)
            .with_client_auth_cert(cert_chain, key)
            .map_err(|e| MTLSError::TlsConfigError { detail: e.to_string() })
    }
}
