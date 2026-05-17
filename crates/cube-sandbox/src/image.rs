use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use sha2::{Sha256, Digest};

use crate::error::SandboxResult;
use openforce_domain::sandbox_image::SandboxImage;

#[derive(Debug, Clone)]
pub struct CachedImage {
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub digest: String,
}

pub struct ImageManager {
    cache_dir: PathBuf,
    cache: RwLock<HashMap<String, CachedImage>>,
}

impl ImageManager {
    pub fn new(cache_dir: &str) -> Self {
        Self { cache_dir: PathBuf::from(cache_dir), cache: RwLock::new(HashMap::new()) }
    }

    pub async fn ensure_image(&self, image: &SandboxImage) -> SandboxResult<CachedImage> {
        if let Some(cached) = self.cache.read().await.get(&image.image_digest) {
            return Ok(cached.clone());
        }
        let kp = self.cache_dir.join(&image.image_digest).join("vmlinux.bin");
        let rp = self.cache_dir.join(&image.image_digest).join("rootfs.ext4");
        if kp.exists() && rp.exists() {
            let c = CachedImage { kernel_path: kp, rootfs_path: rp, digest: image.image_digest.clone() };
            self.cache.write().await.insert(image.image_digest.clone(), c.clone());
            return Ok(c);
        }
        Err(crate::error::SandboxError::ImagePullFailed {
            detail: format!("image not cached: {}", image.canonical_ref()),
        })
    }

    pub fn verify_digest(path: &std::path::Path, expected: &str) -> SandboxResult<()> {
        let data = std::fs::read(path)
            .map_err(|e| crate::error::SandboxError::IoError { source: e })?;
        let mut h = Sha256::new();
        h.update(&data);
        let actual = format!("sha256:{:x}", h.finalize());
        if actual != expected {
            return Err(crate::error::SandboxError::ImageDigestMismatch { expected: expected.into(), actual });
        }
        Ok(())
    }
}
