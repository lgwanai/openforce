use std::collections::HashSet;
use tokio::sync::RwLock;

pub struct NetworkManager {
    allocated_macs: RwLock<HashSet<String>>,
}

impl NetworkManager {
    pub fn new() -> Self { Self { allocated_macs: RwLock::new(HashSet::new()) } }

    pub async fn allocate_mac(&self) -> String {
        loop {
            let rand = ring::rand::SystemRandom::new();
            let mut bytes = [0u8; 4];
            ring::rand::SecureRandom::fill(&rand, &mut bytes).expect("CRNG");
            let mac = format!("06:00:AC:{:02x}:{:02x}:{:02x}", bytes[1], bytes[2], bytes[3]);
            let mut macs = self.allocated_macs.write().await;
            if !macs.contains(&mac) { macs.insert(mac.clone()); return mac; }
        }
    }

    pub async fn release_mac(&self, mac: &str) { self.allocated_macs.write().await.remove(mac); }

    pub fn tap_device_name(&self, vm_id: &str) -> String {
        format!("swarmos-{}", &vm_id[..8.min(vm_id.len())])
    }
}
