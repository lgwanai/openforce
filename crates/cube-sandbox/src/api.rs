use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::{debug, warn};

use crate::config::{BootSource, Drive, MachineConfig, NetworkInterface};
use crate::error::SandboxError;

/// HTTP client for Firecracker's Unix socket REST API using raw Unix sockets.
pub struct FirecrackerClient {
    socket_path: PathBuf,
}

impl FirecrackerClient {
    pub fn new(socket_path: &str) -> Self {
        Self { socket_path: PathBuf::from(socket_path) }
    }

    pub async fn put_machine_config(&self, config: &MachineConfig) -> Result<(), SandboxError> {
        let body = serde_json::to_vec(config)
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;
        self.put("/machine-config", &body).await
    }

    pub async fn put_boot_source(&self, source: &BootSource) -> Result<(), SandboxError> {
        let body = serde_json::to_vec(source)
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;
        self.put("/boot-source", &body).await
    }

    pub async fn put_drive(&self, drive: &Drive) -> Result<(), SandboxError> {
        let body = serde_json::to_vec(drive)
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;
        self.put(&format!("/drives/{}", drive.drive_id), &body).await
    }

    pub async fn put_network_interface(&self, iface: &NetworkInterface) -> Result<(), SandboxError> {
        let body = serde_json::to_vec(iface)
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;
        self.put(&format!("/network-interfaces/{}", iface.iface_id), &body).await
    }

    pub async fn instance_start(&self) -> Result<(), SandboxError> {
        self.put("/actions", b"{\"action_type\":\"InstanceStart\"}").await
    }

    async fn put(&self, path: &str, body: &[u8]) -> Result<(), SandboxError> {
        let mut stream = UnixStream::connect(&self.socket_path).await
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;

        let request = format!(
            "PUT {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            body.len()
        );

        let mut full_request = request.into_bytes();
        full_request.extend_from_slice(body);

        stream.write_all(&full_request).await
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await
            .map_err(|e| SandboxError::ApiError { detail: e.to_string() })?;

        let resp_str = String::from_utf8_lossy(&response);
        if resp_str.contains("204 No Content") || resp_str.contains("200 OK") {
            debug!("firecracker api: {path} OK");
            Ok(())
        } else {
            let first_line = resp_str.lines().next().unwrap_or("unknown");
            warn!("firecracker api error: {path} -> {first_line}");
            Err(SandboxError::ApiError { detail: format!("HTTP {first_line}") })
        }
    }
}
