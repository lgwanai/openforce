use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Workspace snapshot — frozen state of project files at a point in time.
/// Architecture doc section 10: Project Workspace as VFS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub snapshot_id: Uuid,
    pub session_id: Uuid,
    pub parent_snapshot_id: Option<Uuid>,
    pub files: Vec<SnapshotFile>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub path: String,
    pub content_sha256: String,
    pub size_bytes: u64,
}

impl WorkspaceSnapshot {
    pub fn new(session_id: Uuid, parent: Option<Uuid>) -> Self {
        Self {
            snapshot_id: Uuid::now_v7(),
            session_id,
            parent_snapshot_id: parent,
            files: vec![],
            created_at: Utc::now(),
        }
    }

    pub fn diff(&self, other: &WorkspaceSnapshot) -> Vec<String> {
        let self_paths: std::collections::HashSet<_> = self.files.iter().map(|f| &f.path).collect();
        let other_paths: std::collections::HashSet<_> = other.files.iter().map(|f| &f.path).collect();
        let mut changed: Vec<String> = self_paths.symmetric_difference(&other_paths).map(|p| p.to_string()).collect();
        // Also check files in both but with different hash
        for f in &self.files {
            if let Some(of) = other.files.iter().find(|of| of.path == f.path) {
                if f.content_sha256 != of.content_sha256 {
                    changed.push(f.path.clone());
                }
            }
        }
        changed
    }
}
