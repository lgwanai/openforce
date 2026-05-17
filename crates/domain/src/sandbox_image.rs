use serde::{Deserialize, Serialize};

/// Architecture doc section 10: three sandbox classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxImageClass {
    /// Minimal agent runtime (Python/Node/Go + tool support)
    AgentSpace,
    /// Full integration target (Node + Go + DB drivers + test frameworks)
    TargetFullstack,
    /// GPU-accelerated target (CUDA + ML frameworks)
    TargetGpu,
}

impl SandboxImageClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentSpace => "agent-space",
            Self::TargetFullstack => "target-fullstack",
            Self::TargetGpu => "target-gpu",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent-space" => Some(Self::AgentSpace),
            "target-fullstack" => Some(Self::TargetFullstack),
            "target-gpu" => Some(Self::TargetGpu),
            _ => None,
        }
    }

    /// Pool key for WarmPool lookups.
    pub fn pool_key(&self) -> &'static str {
        match self {
            Self::AgentSpace => "agent",
            Self::TargetFullstack => "fullstack",
            Self::TargetGpu => "gpu",
        }
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
        format!("{}/{}@{}", self.registry, self.rootfs_image, self.image_digest)
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
