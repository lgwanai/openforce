use crate::error::SandboxResult;

pub struct SnapshotManager { snapshot_dir: String }

impl SnapshotManager {
    pub fn new(snapshot_dir: &str) -> Self { Self { snapshot_dir: snapshot_dir.into() } }
    pub fn snapshot_path(&self, snapshot_id: &str) -> String {
        format!("{}/{snapshot_id}.snap", self.snapshot_dir)
    }
    pub fn snapshot_exists(&self, snapshot_id: &str) -> bool {
        std::path::Path::new(&self.snapshot_path(snapshot_id)).exists()
    }
}
