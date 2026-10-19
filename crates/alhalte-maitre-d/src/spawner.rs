//! Banquette process and server spawning abstractions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alhalte_banquette::{BanquetteServer, CellarSink, InMemoryCellarSink, Room, ServerConfig};
use async_trait::async_trait;
use tin::TinId;
use tokio::sync::{Mutex, watch};

use crate::error::{MaitreDError, Result};

/// Trait defining how `alhalte-maitre-d` launches room daemon instances.
#[async_trait]
pub trait BanquetteSpawner: Send + Sync {
    /// Launches or prepares a `banquette` instance for `room_id` and initial administrator `admin_account`.
    ///
    /// `room_head` is the persisted `RoomData` tin ID read from `room_heads`.
    /// When it is `Some`, the room must be restored from that tin; a missing,
    /// corrupt, or mismatched head is an error and the spawner must never fall
    /// back to an empty room. `None` starts a brand-new room.
    ///
    /// Returns the UNIX domain socket path where the `banquette` server is listening.
    async fn spawn_banquette(
        &self,
        room_id: u64,
        admin_account: u64,
        room_head: Option<TinId>,
    ) -> Result<PathBuf>;
}

/// Spawns external `alhalte-banquette` executable binaries as OS child processes.
///
/// The service token is redacted from the [`std::fmt::Debug`] representation and
/// never appears in the child's argument list: maitre-d forwards the
/// `--service-token-file` path it received, or passes the value through the
/// `ALHALTE_SERVICE_TOKEN` environment variable (Issue #194).
#[derive(Clone)]
pub struct ProcessBanquetteSpawner {
    binary_path: PathBuf,
    socket_dir: PathBuf,
    cellar_url: String,
    chunk_size_exponent: u8,
    service_token: Option<String>,
    service_token_file: Option<PathBuf>,
}

impl std::fmt::Debug for ProcessBanquetteSpawner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessBanquetteSpawner")
            .field("binary_path", &self.binary_path)
            .field("socket_dir", &self.socket_dir)
            .field("cellar_url", &self.cellar_url)
            .field("chunk_size_exponent", &self.chunk_size_exponent)
            .field(
                "service_token",
                &self.service_token.as_ref().map(|_| "<redacted>"),
            )
            .field("service_token_file", &self.service_token_file)
            .finish()
    }
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
            service_token: None,
            service_token_file: None,
        }
    }

    /// Configures the internal cellar service token passed to spawned banquette processes.
    ///
    /// An empty token is treated as unset. The value is never logged and never
    /// placed on the child's command line: it is passed through the
    /// `ALHALTE_SERVICE_TOKEN` environment variable.
    #[must_use]
    pub fn with_service_token(mut self, token: impl Into<String>) -> Self {
        let token = token.into();
        self.service_token = if token.trim().is_empty() {
            None
        } else {
            Some(token)
        };
        self
    }

    /// Configures the `0600` file holding the internal service token.
    ///
    /// The path is forwarded to the spawned banquette as `--service-token-file`,
    /// so neither the parent nor the child exposes the value in `ps`.
    #[must_use]
    pub fn with_service_token_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.service_token_file = Some(path.into());
        self
    }

    /// Builds the argument list passed to the spawned `alhalte-banquette` binary.
    ///
    /// Exposed so tests can assert the `--room-tin` restore contract and the
    /// "no secret in argv" property without launching a child process.
    pub fn command_args(
        &self,
        room_id: u64,
        admin_account: u64,
        socket_path: &std::path::Path,
        room_head: Option<&TinId>,
    ) -> Vec<String> {
        let mut args = vec![
            "--room-id".to_string(),
            format!("{room_id:016x}"),
            "--admin".to_string(),
            admin_account.to_string(),
            "--socket".to_string(),
            socket_path.display().to_string(),
            "--cellar-url".to_string(),
            self.cellar_url.clone(),
            "--chunk-size-exponent".to_string(),
            self.chunk_size_exponent.to_string(),
        ];

        if let Some(tin_id) = room_head {
            args.push("--room-tin".to_string());
            args.push(hex::encode(tin_id));
        }

        if let Some(path) = self.service_token_file.as_deref() {
            args.push("--service-token-file".to_string());
            args.push(path.display().to_string());
        }

        args
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
    async fn spawn_banquette(
        &self,
        room_id: u64,
        admin_account: u64,
        room_head: Option<TinId>,
    ) -> Result<PathBuf> {
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
        cmd.args(self.command_args(room_id, admin_account, &socket_path, room_head.as_ref()));
        // The token never goes on the command line. When only the value is known
        // (no forwarded file path) hand it to the child through the environment.
        if self.service_token_file.is_none()
            && let Some(token) = self.service_token.as_deref()
        {
            cmd.env("ALHALTE_SERVICE_TOKEN", token);
        }

        cmd.spawn().map_err(|e| {
            MaitreDError::Banquette(format!(
                "Failed to spawn banquette binary at {:?} (resolved: {:?}): {e}",
                self.binary_path, resolved_bin
            ))
        })?;

        Ok(socket_path)
    }
}

/// A running in-process room server together with its graceful shutdown handle.
struct InProcessRoom {
    shutdown: watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
}

/// Spawns in-process [`BanquetteServer`] instances for automated testing without child processes.
#[derive(Clone)]
pub struct InProcessBanquetteSpawner {
    socket_dir: PathBuf,
    cellar: Arc<dyn CellarSink>,
    chunk_size_exponent: u8,
    heartbeat_interval: Duration,
    rooms: Arc<Mutex<HashMap<u64, InProcessRoom>>>,
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
            rooms: Arc::new(Mutex::new(HashMap::new())),
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

    /// Overrides the heartbeat interval of spawned in-process servers.
    ///
    /// Mostly useful for tests that need the periodic persist (and therefore
    /// the reported room head) to happen quickly.
    #[must_use]
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// Requests a graceful shutdown of the in-process room for `room_id`.
    ///
    /// The room persists its final state before the server returns. Returns
    /// `false` when no server is tracked for the room.
    pub async fn shutdown_room(&self, room_id: u64) -> bool {
        let entry = {
            let mut rooms = self.rooms.lock().await;
            rooms.remove(&room_id)
        };
        match entry {
            Some(entry) => {
                let _ = entry.shutdown.send(true);
                let _ = entry.handle.await;
                true
            }
            None => false,
        }
    }

    /// Requests a graceful shutdown of every in-process room.
    pub async fn shutdown_all(&self) {
        let entries = {
            let mut rooms = self.rooms.lock().await;
            std::mem::take(&mut *rooms)
        };
        for (_, entry) in entries {
            let _ = entry.shutdown.send(true);
            let _ = entry.handle.await;
        }
    }
}

#[async_trait]
impl BanquetteSpawner for InProcessBanquetteSpawner {
    async fn spawn_banquette(
        &self,
        room_id: u64,
        admin_account: u64,
        room_head: Option<TinId>,
    ) -> Result<PathBuf> {
        tokio::fs::create_dir_all(&self.socket_dir)
            .await
            .map_err(MaitreDError::Io)?;

        let socket_path = self
            .socket_dir
            .join(format!("banquette-inproc-{room_id}.sock"));

        if socket_path.exists() {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        // A stale in-process server for the same room would fight over the same
        // socket path and head pointer, so stop it before starting the new one.
        self.shutdown_room(room_id).await;

        let room = match room_head {
            Some(tin_id) => Room::load_from_cellar(self.cellar.as_ref(), &tin_id, room_id)
                .await
                .map_err(|e| {
                    MaitreDError::Banquette(format!(
                        "failed to restore room {room_id} from persisted tin {}: {e}",
                        hex::encode(tin_id)
                    ))
                })?,
            None => Room::new(room_id, admin_account, self.chunk_size_exponent).map_err(|e| {
                MaitreDError::Banquette(format!("Failed to create Room data: {e:?}"))
            })?,
        };

        let config = ServerConfig {
            socket_path: socket_path.clone(),
            heartbeat_interval: self.heartbeat_interval,
            admin_account,
        };

        let server = BanquetteServer::new(config, room, Arc::clone(&self.cellar));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = tokio::spawn(async move {
            if let Err(e) = server.run_until_shutdown(shutdown_rx).await {
                tracing::error!("In-process BanquetteServer error: {e:?}");
            }
        });

        self.rooms.lock().await.insert(
            room_id,
            InProcessRoom {
                shutdown: shutdown_tx,
                handle,
            },
        );

        Ok(socket_path)
    }
}
