use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfig {
    pub vcpu_count: i32,
    pub mem_size_mib: i32,
    pub smt: bool,
    pub track_dirty_pages: bool,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self { vcpu_count: 2, mem_size_mib: 4096, smt: false, track_dirty_pages: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSource {
    pub kernel_image_path: String,
    pub boot_args: String,
    pub initrd_path: Option<String>,
}

impl BootSource {
    pub fn new(kernel_image_path: &str) -> Self {
        Self {
            kernel_image_path: kernel_image_path.into(),
            boot_args: "console=ttyS0 reboot=k panic=1 pci=off".into(),
            initrd_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    pub drive_id: String,
    pub path_on_host: String,
    pub is_root_device: bool,
    pub is_read_only: bool,
    pub partuuid: Option<String>,
    pub rate_limiter: Option<RateLimiter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: String,
    pub rx_rate_limiter: Option<RateLimiter>,
    pub tx_rate_limiter: Option<RateLimiter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiter {
    pub bandwidth: Option<TokenBucket>,
    pub ops: Option<TokenBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBucket {
    pub size: u64,
    pub one_time_burst: Option<u64>,
    pub refill_time: u64,
}

#[derive(Debug, Clone)]
pub struct VmConfig {
    pub vm_id: String,
    pub session_id: String,
    pub task_id: String,
    pub sandbox_class: String,
    pub machine: MachineConfig,
    pub boot_source: BootSource,
    pub root_drive: Drive,
    pub net_iface: NetworkInterface,
    pub socket_path: String,
    pub log_path: String,
}

impl VmConfig {
    pub fn new(
        vm_id: String, session_id: String, task_id: String, sandbox_class: String,
        kernel_path: &str, rootfs_path: &str, tap_device: &str, mac: &str,
    ) -> Self {
        let socket_path = format!("/var/run/swarmos/vms/{vm_id}.sock");
        let log_path = format!("/var/log/swarmos/vms/{vm_id}.log");
        Self {
            vm_id, session_id, task_id, sandbox_class,
            machine: MachineConfig::default(),
            boot_source: BootSource::new(kernel_path),
            root_drive: Drive {
                drive_id: "rootfs".into(), path_on_host: rootfs_path.into(),
                is_root_device: true, is_read_only: true, partuuid: None, rate_limiter: None,
            },
            net_iface: NetworkInterface {
                iface_id: "eth0".into(), host_dev_name: tap_device.into(),
                guest_mac: mac.into(), rx_rate_limiter: None, tx_rate_limiter: None,
            },
            socket_path, log_path,
        }
    }
}
