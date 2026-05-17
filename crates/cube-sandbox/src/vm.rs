use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::info;

use crate::api::FirecrackerClient;
use crate::config::VmConfig;
use crate::error::{SandboxError, SandboxResult};
use openforce_domain::sandbox_image::VmState;

#[derive(Debug, Clone)]
pub struct SandboxVM {
    pub vm_id: String,
    pub session_id: String,
    pub task_id: String,
    pub sandbox_class: String,
    pub state: VmState,
    pub pid: Option<u32>,
    pub socket_path: String,
}

/// Manages Firecracker MicroVM lifecycle.
pub struct VmManager {
    firecracker_binary: String,
    active: Arc<RwLock<HashMap<String, SandboxVM>>>,
}

impl VmManager {
    pub fn new(firecracker_binary: &str) -> Self {
        Self { firecracker_binary: firecracker_binary.into(), active: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn create(&self, config: VmConfig) -> SandboxResult<SandboxVM> {
        let socket_path = config.socket_path.clone();
        let log_path = config.log_path.clone();
        let _ = std::fs::remove_file(&socket_path);

        let child = Command::new(&self.firecracker_binary)
            .arg("--api-sock").arg(&socket_path)
            .arg("--log-path").arg(&log_path)
            .arg("--level").arg("Info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SandboxError::ApiError { detail: format!("spawn firecracker: {e}") })?;

        let pid = child.id().expect("child must have pid");

        // Wait for API socket
        let mut attempts = 0u32;
        loop {
            if std::path::Path::new(&socket_path).exists() { break; }
            if attempts > 100 {
                return Err(SandboxError::Timeout { detail: "firecracker socket timeout".into() });
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            attempts += 1;
        }

        let client = FirecrackerClient::new(&socket_path);
        client.put_machine_config(&config.machine).await?;
        client.put_boot_source(&config.boot_source).await?;
        client.put_drive(&config.root_drive).await?;
        client.put_network_interface(&config.net_iface).await?;

        let vm = SandboxVM {
            vm_id: config.vm_id.clone(),
            session_id: config.session_id,
            task_id: config.task_id,
            sandbox_class: config.sandbox_class,
            state: VmState::Creating,
            pid: Some(pid),
            socket_path,
        };

        self.active.write().await.insert(config.vm_id, vm.clone());
        info!("VM created: {} pid={pid}", vm.vm_id);
        Ok(vm)
    }

    pub async fn start(&self, vm_id: &str) -> SandboxResult<VmState> {
        let vm = self.get(vm_id).await?;
        let client = FirecrackerClient::new(&vm.socket_path);
        client.instance_start().await?;
        let mut active = self.active.write().await;
        if let Some(v) = active.get_mut(vm_id) { v.state = VmState::Running; }
        info!("VM started: {vm_id}");
        Ok(VmState::Running)
    }

    pub async fn destroy(&self, vm_id: &str) -> SandboxResult<()> {
        let vm = self.get(vm_id).await?;
        if let Some(pid) = vm.pid { unsafe { libc::kill(pid as i32, libc::SIGTERM); } }
        let _ = std::fs::remove_file(&vm.socket_path);
        self.active.write().await.remove(vm_id);
        info!("VM destroyed: {vm_id}");
        Ok(())
    }

    pub async fn get(&self, vm_id: &str) -> SandboxResult<SandboxVM> {
        self.active.read().await.get(vm_id).cloned()
            .ok_or_else(|| SandboxError::VmNotFound { vm_id: vm_id.into() })
    }

    pub async fn list(&self) -> Vec<SandboxVM> {
        self.active.read().await.values().cloned().collect()
    }
}
