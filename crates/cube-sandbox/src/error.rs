use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("firecracker api error: {detail}")]
    ApiError { detail: String },
    #[error("vm not found: {vm_id}")]
    VmNotFound { vm_id: String },
    #[error("vm already running: {vm_id}")]
    VmAlreadyRunning { vm_id: String },
    #[error("image pull failed: {detail}")]
    ImagePullFailed { detail: String },
    #[error("image digest mismatch: expected {expected}, got {actual}")]
    ImageDigestMismatch { expected: String, actual: String },
    #[error("network error: {detail}")]
    NetworkError { detail: String },
    #[error("credential injection failed: {detail}")]
    CredentialError { detail: String },
    #[error("snapshot error: {detail}")]
    SnapshotError { detail: String },
    #[error("io error: {source}")]
    IoError { #[from] source: std::io::Error },
    #[error("timeout: {detail}")]
    Timeout { detail: String },
}

pub type SandboxResult<T> = Result<T, SandboxError>;
