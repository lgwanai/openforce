use std::sync::Arc;
use uuid::Uuid;
use tracing::info;

use openforce_domain::sandbox_image::{SandboxImage, SandboxImageClass, VmState};
use openforce_cube_sandbox::vm::{VmManager, SandboxVM};
use openforce_cube_sandbox::image::ImageManager;
use openforce_cube_sandbox::config::VmConfig;
use openforce_cube_sandbox::credentials::{CredentialInjector, TempCredential};

#[derive(Debug, Clone)]
pub struct TempCred {
    pub key: String,
    pub value: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct Sandbox {
    pub id: Uuid,
    pub session_id: Uuid,
    pub task_id: Option<Uuid>,
    pub sandbox_class: String,
    pub image_digest: String,
    pub state: VmState,
}

pub struct SandboxManager {
    vm_manager: Arc<VmManager>,
    image_manager: Arc<ImageManager>,
    credential_injector: CredentialInjector,
}

impl SandboxManager {
    pub fn new(
        vm_manager: Arc<VmManager>,
        image_manager: Arc<ImageManager>,
        metadata_dir: &str,
    ) -> Self {
        Self { vm_manager, image_manager, credential_injector: CredentialInjector::new(metadata_dir) }
    }

    pub async fn create(
        &self, session_id: Uuid, task_id: Option<Uuid>,
        class: SandboxImageClass, image: &SandboxImage,
    ) -> Result<Sandbox, String> {
        let vm_id = Uuid::now_v7().to_string();
        let cached = self.image_manager.ensure_image(image).await
            .map_err(|e| format!("image: {e}"))?;

        let vm_config = VmConfig::new(
            vm_id.clone(), session_id.to_string(),
            task_id.map(|t| t.to_string()).unwrap_or_default(),
            class.as_str().into(),
            cached.kernel_path.to_str().unwrap_or("/dev/null"),
            cached.rootfs_path.to_str().unwrap_or("/dev/null"),
            "swarmos-tap", "06:00:AC:10:00:01",
        );

        self.vm_manager.create(vm_config).await.map_err(|e| format!("vm: {e}"))?;

        let sb = Sandbox {
            id: Uuid::parse_str(&vm_id).unwrap_or(Uuid::nil()),
            session_id, task_id,
            sandbox_class: class.as_str().into(),
            image_digest: image.image_digest.clone(),
            state: VmState::Creating,
        };
        info!("sandbox created: id={} session={session_id}", sb.id);
        Ok(sb)
    }

    pub async fn start(&self, sandbox_id: Uuid) -> Result<VmState, String> {
        self.vm_manager.start(&sandbox_id.to_string()).await
            .map_err(|e| format!("vm start: {e}"))
    }

    pub async fn destroy(&self, sandbox_id: Uuid) -> Result<(), String> {
        let id = sandbox_id.to_string();
        self.credential_injector.cleanup(&id).ok();
        self.vm_manager.destroy(&id).await.map_err(|e| format!("vm destroy: {e}"))
    }

    pub async fn inject_credentials(&self, sandbox_id: Uuid, creds: Vec<TempCred>) -> Result<(), String> {
        let tc: Vec<TempCredential> = creds.iter().map(|c| TempCredential {
            key: c.key.clone(), value: c.value.clone(), expires_at: c.expires_at.to_rfc3339(),
        }).collect();
        self.credential_injector.inject(&sandbox_id.to_string(), &tc)
            .map_err(|e| format!("cred: {e}"))
    }

    pub async fn get(&self, sandbox_id: Uuid) -> Option<Sandbox> {
        self.vm_manager.get(&sandbox_id.to_string()).await.ok().map(|vm| Sandbox {
            id: Uuid::parse_str(&vm.vm_id).unwrap_or(Uuid::nil()),
            session_id: Uuid::parse_str(&vm.session_id).unwrap_or(Uuid::nil()),
            task_id: vm.task_id.is_empty().then(Uuid::nil),
            sandbox_class: vm.sandbox_class,
            image_digest: String::new(),
            state: vm.state,
        })
    }

    pub async fn list_active(&self) -> Vec<Sandbox> {
        self.vm_manager.list().await.into_iter().map(|vm| Sandbox {
            id: Uuid::parse_str(&vm.vm_id).unwrap_or(Uuid::nil()),
            session_id: Uuid::parse_str(&vm.session_id).unwrap_or(Uuid::nil()),
            task_id: vm.task_id.is_empty().then(Uuid::nil),
            sandbox_class: vm.sandbox_class,
            image_digest: String::new(),
            state: vm.state,
        }).collect()
    }
}
