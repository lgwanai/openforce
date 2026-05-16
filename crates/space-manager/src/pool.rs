use std::collections::VecDeque;
use uuid::Uuid;

/// Warm pool for pre-provisioned sandboxes to reduce cold start latency.
/// Architecture doc section 10.4: layered images, temporary credentials.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub agent_count: usize,
    pub target_fullstack_count: usize,
    pub target_gpu_count: usize,
}

impl Default for PoolConfig {
    fn default() -> Self { Self { agent_count: 5, target_fullstack_count: 3, target_gpu_count: 1 } }
}

pub struct WarmPool {
    config: PoolConfig,
    agent_pool: VecDeque<Uuid>,
    fullstack_pool: VecDeque<Uuid>,
    gpu_pool: VecDeque<Uuid>,
}

impl WarmPool {
    pub fn new(config: PoolConfig) -> Self {
        Self { config, agent_pool: VecDeque::new(), fullstack_pool: VecDeque::new(), gpu_pool: VecDeque::new() }
    }

    pub fn add(&mut self, class: &str, sandbox_id: Uuid) {
        match class {
            "agent" => self.agent_pool.push_back(sandbox_id),
            "fullstack" => self.fullstack_pool.push_back(sandbox_id),
            "gpu" => self.gpu_pool.push_back(sandbox_id),
            _ => {}
        }
    }

    pub fn take(&mut self, class: &str) -> Option<Uuid> {
        match class {
            "agent" => self.agent_pool.pop_front(),
            "fullstack" => self.fullstack_pool.pop_front(),
            "gpu" => self.gpu_pool.pop_front(),
            _ => None,
        }
    }

    pub fn agent_needed(&self) -> bool {
        self.agent_pool.len() < self.config.agent_count
    }

    pub fn fullstack_needed(&self) -> bool {
        self.fullstack_pool.len() < self.config.target_fullstack_count
    }

    pub fn gpu_needed(&self) -> bool {
        self.gpu_pool.len() < self.config.target_gpu_count
    }
}
