//! Banquette process and server spawning abstractions.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alhalte_banquette::{BanquetteServer, CellarSink, InMemoryCellarSink, Room, ServerConfig};
use async_trait::async_trait;

use crate::error::{MaitreDError, Result};

/// Trait defining how `alhalte-maitre-d` launches room daemon instances.
#[async_trait]
pub trait BanquetteSpawner: Send + Sync {
    /// Launches or prepares a `banquette` instance for `room_id` and initial administrator `admin_account`.
    ///
    /// Returns the UNIX domain socket path where the `banquette` server is listening.
    async fn spawn_banquette(&self, room_id: u64, admin_account: u64) -> Result<PathBuf>;
}

/// Spawns external `alhalte-banquette` executable binaries as OS child processes.
#[derive(Debug, Clone)]
pub struct ProcessBanquetteSpawner {
    binary_path: PathBuf,
    socket_dir: PathBuf,
    cellar_url: String,
    chunk_size_exponent: u8,
}

impl ProcessBanquetteSpawner {
    /// Creates a new [`ProcessBanquetteSpawner`].
    pub fn new(
        binary_path: impl Into<PathBuf>,
        socket_dir: impl Into<PathBuf>,
        cellar_url: impl Into<String>,
        chunk_size_exponent: u8,
    ) -> Self {
        Self {
            binary_path: binary_path.into(),
            socket_dir: socket_dir.into(),
            cellar_url: cellar_url.into(),
            chunk_size_exponent,
        }
    }
}

/// Resolves the filesystem path to the `alhalte-banquette` executable binary.
///
/// If `binary_path` exists directly on disk, it is returned as-is.
/// Otherwise, checks:
/// 1. `target/debug/<bin_name>` relative to the current working directory
/// 2. `target/release/<bin_name>` relative to the current working directory
/// 3. Alongside the current executable (`std::env::current_exe()`)
/// 4. Directories listed in the `PATH` environment variable
///
/// If not found in any candidate locations, returns `binary_path.to_path_buf()` as fallback.
pub fn resolve_banquette_bin(binary_path: &std::path::Path) -> PathBuf {
    if binary_path.is_file() {
        return binary_path.to_path_buf();
    }

    let bin_name = binary_path.file_name().unwrap_or(binary_path.as_os_str());

    // 1. Current working directory target/debug and target/release
    let candidates = [
        PathBuf::from("target/debug").join(bin_name),
        PathBuf::from("target/release").join(bin_name),
    ];
    for candidate in &candidates {
        if candidate.is_file() {
            return candidate.clone();
        }
    }

    // 2. Alongside the currently running executable
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        let candidate = parent.join(bin_name);
        if candidate.is_file() {
            return candidate;
        }
    }

    // 3. Search in PATH
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(bin_name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    binary_path.to_path_buf()
}

#[async_trait]
impl BanquetteSpawner for ProcessBanquetteSpawner {
    async fn spawn_banquette(&self, room_id: u64, admin_account: u64) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.socket_dir)
            .await
            .map_err(MaitreDError::Io)?;

        let socket_path = self.socket_dir.join(format!("banquette-{room_id}.sock"));

        if socket_path.exists() {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        let resolved_bin = resolve_banquette_bin(&self.binary_path);
        tracing::debug!(
            "Spawning banquette for room {room_id} with binary: {:?}",
            resolved_bin
        );

        let mut cmd = tokio::process::Command::new(&resolved_bin);
        cmd.arg("--room-id")
            .arg(room_id.to_string())
            .arg("--admin")
            .arg(admin_account.to_string())
            .arg("--socket")
            .arg(&socket_path)
            .arg("--cellar-url")
            .arg(&self.cellar_url)
            .arg("--chunk-size-exponent")
            .arg(self.chunk_size_exponent.to_string());

        cmd.spawn().map_err(|e| {
            MaitreDError::Banquette(format!(
                "Failed to spawn banquette binary at {:?} (resolved: {:?}): {e}",
                self.binary_path, resolved_bin
            ))
        })?;

        Ok(socket_path)
    }
}

/// Spawns in-process [`BanquetteServer`] instances for automated testing without child processes.
#[derive(Clone)]
pub struct InProcessBanquetteSpawner {
    socket_dir: PathBuf,
    cellar: Arc<dyn CellarSink>,
    chunk_size_exponent: u8,
    heartbeat_interval: Duration,
}

impl InProcessBanquetteSpawner {
    /// Creates a new [`InProcessBanquetteSpawner`].
    pub fn new(
        socket_dir: impl Into<PathBuf>,
        cellar: Arc<dyn CellarSink>,
        chunk_size_exponent: u8,
    ) -> Self {
        Self {
            socket_dir: socket_dir.into(),
            cellar,
            chunk_size_exponent,
            heartbeat_interval: Duration::from_secs(240),
        }
    }

    /// Creates an in-process spawner using [`InMemoryCellarSink`].
    pub fn new_with_in_memory_cellar(
        socket_dir: impl Into<PathBuf>,
        chunk_size_exponent: u8,
    ) -> Self {
        Self::new(
            socket_dir,
            Arc::new(InMemoryCellarSink::new()),
            chunk_size_exponent,
        )
    }
}

#[async_trait]
impl BanquetteSpawner for InProcessBanquetteSpawner {
    async fn spawn_banquette(&self, room_id: u64, admin_account: u64) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.socket_dir)
            .await
            .map_err(MaitreDError::Io)?;

        let socket_path = self
            .socket_dir
            .join(format!("banquette-inproc-{room_id}.sock"));

        if socket_path.exists() {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        let room = Room::new(room_id, admin_account, self.chunk_size_exponent)
            .map_err(|e| MaitreDError::Banquette(format!("Failed to create Room data: {e:?}")))?;

        let config = ServerConfig {
            socket_path: socket_path.clone(),
            heartbeat_interval: self.heartbeat_interval,
            admin_account,
            system_session_id: 1,
        };

        let server = BanquetteServer::new(config, room, Arc::clone(&self.cellar));

        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                tracing::error!("In-process BanquetteServer error: {e:?}");
            }
        });

        Ok(socket_path)
    }
}
