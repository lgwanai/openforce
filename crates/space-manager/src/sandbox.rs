use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxClass {
    AgentSpace,
    ProjectWorkspace,
    ExecutionTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxState {
    Creating, Ready, Running, TearingDown, Destroyed,
}

#[derive(Debug, Clone)]
pub struct Sandbox {
    pub id: Uuid,
    pub session_id: Uuid,
    pub task_id: Option<Uuid>,
    pub class: SandboxClass,
    pub image_digest: String,
    pub state: SandboxState,
    pub credentials: Vec<TempCredential>,
}

#[derive(Debug, Clone)]
pub struct TempCredential {
    pub key: String,
    pub value: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct SandboxManager {
    active: HashMap<Uuid, Sandbox>,
}

impl SandboxManager {
    pub fn new() -> Self { Self { active: HashMap::new() } }

    pub fn create(&mut self, session_id: Uuid, class: SandboxClass, image: &str) -> Sandbox {
        let sb = Sandbox {
            id: Uuid::now_v7(), session_id, task_id: None,
            class, image_digest: image.into(),
            state: SandboxState::Creating, credentials: vec![],
        };
        self.active.insert(sb.id, sb.clone());
        sb
    }

    pub fn destroy(&mut self, id: Uuid) {
        if let Some(sb) = self.active.get_mut(&id) {
            sb.state = SandboxState::Destroyed;
            sb.credentials.clear();
        }
        self.active.remove(&id);
    }

    pub fn inject_credentials(&mut self, id: Uuid, creds: Vec<TempCredential>) {
        if let Some(sb) = self.active.get_mut(&id) {
            sb.credentials = creds;
        }
    }
}
