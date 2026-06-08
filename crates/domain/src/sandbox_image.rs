use serde::{Deserialize, Serialize};

/// Sandbox image class — now a string-based type so that platform operators
/// can define custom sandbox classes beyond the built-in three.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SandboxImageClass(String);

impl SandboxImageClass {
    // Built-in sandbox classes (architecture doc section 10)
    /// Minimal agent runtime (Python/Node/Go + tool support)
    pub fn agent_space() -> Self {
        Self("agent-space".into())
    }
    /// Full integration target (Node + Go + DB drivers + test frameworks)
    pub fn target_fullstack() -> Self {
        Self("target-fullstack".into())
    }
    /// GPU-accelerated target (CUDA + ML frameworks)
    pub fn target_gpu() -> Self {
        Self("target-gpu".into())
    }

    /// Create a custom image class from any string
    pub fn custom(name: &str) -> Self {
        Self(name.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(Self(s.to_lowercase()))
    }

    /// Pool key for WarmPool lookups.
    pub fn pool_key(&self) -> &str {
        match self.0.as_str() {
            "agent-space" => "agent",
            "target-fullstack" => "fullstack",
            "target-gpu" => "gpu",
            other => other,
        }
    }
}

impl Default for SandboxImageClass {
    fn default() -> Self {
        Self::agent_space()
    }
}

impl std::fmt::Display for SandboxImageClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Immutable reference to a sandbox image (architecture doc section 22.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxImage {
    pub image_digest: String,
    pub registry: String,
    pub kernel_image: String,
    pub rootfs_image: String,
    pub class: SandboxImageClass,
}

impl SandboxImage {
    pub fn canonical_ref(&self) -> String {
        format!(
            "{}/{}@{}",
            self.registry, self.rootfs_image, self.image_digest
        )
    }
}

/// Runtime state of a Firecracker MicroVM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmState {
    Creating,
    Starting,
    Running,
    Stopping,
    Stopped,
    Destroyed,
}
