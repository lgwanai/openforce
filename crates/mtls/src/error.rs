use thiserror::Error;

#[derive(Debug, Error)]
pub enum MTLSError {
    #[error("ca error: {detail}")]
    CaError { detail: String },
    #[error("certificate verification failed: {detail}")]
    VerificationFailed { detail: String },
    #[error("certificate expired: {serial}")]
    CertificateExpired { serial: String },
    #[error("certificate revoked: {serial}")]
    CertificateRevoked { serial: String },
    #[error("role mismatch: expected {expected}, got {actual}")]
    RoleMismatch { expected: String, actual: String },
    #[error("identity extraction failed: {detail}")]
    IdentityExtractionFailed { detail: String },
    #[error("tls config error: {detail}")]
    TlsConfigError { detail: String },
    #[error("rotation error: {detail}")]
    RotationError { detail: String },
    #[error("io error: {source}")]
    IoError { #[from] source: std::io::Error },
}

pub type MTLSResult<T> = Result<T, MTLSError>;
