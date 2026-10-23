//! Banquette process and server spawning abstractions.

use std::path::PathBuf;
#[cfg(unix)]
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{MaitreDError, Result};

#[cfg(unix)]
use alhalte_banquette::{BanquetteServer, CellarSink, InMemoryCellarSink, Room, ServerConfig};
#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use std::time::Duration;
#[cfg(unix)]
use tokio::sync::{Mutex, watch};

/// How `alhalte-maitre-d` reaches a room's `alhalte-banquette` process.
///
/// The protocol codec is transport-independent. Unix uses a UNIX domain socket;
/// Windows uses a named pipe because Tokio 1.54 does not expose AF_UNIX there
/// (`tokio::net::UnixStream` is `cfg_net_unix!`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BanquetteEndpoint {
    /// UNIX domain socket path (Unix).
    #[cfg(unix)]
    UnixSocket(PathBuf),
    /// Windows named-pipe path (`\\.\pipe\...`).
    #[cfg(windows)]
    NamedPipe(String),
}

impl BanquetteEndpoint {
    /// Returns a human-readable description for logs and errors.
    pub fn describe(&self) -> String {
        match self {
            #[cfg(unix)]
            Self::UnixSocket(path) => format!("UNIX socket {}", path.display()),
            #[cfg(windows)]
            Self::NamedPipe(name) => format!("named pipe {name}"),
        }
    }
}

/// Trait defining how `alhalte-maitre-d` launches room daemon instances.
#[async_trait]
pub trait BanquetteSpawner: Send + Sync {
    /// Launches or prepares a `banquette` instance for `room_id` and initial administrator `admin_account`.
    ///
    /// No room head is passed: banquette reads cellar's `room/<room_id>/head`
    /// alias and restores the room itself (Issue #192). A head that exists but
    /// cannot be downloaded, decoded, validated, or that belongs to another room
    /// ID is an error, so a spawner must never fall back to an empty room. With
    /// no alias, banquette starts a brand-new room.
    ///
    /// Returns the local transport endpoint where the `banquette` server listens.
    async fn spawn_banquette(&self, room_id: u64, admin_account: u64) -> Result<BanquetteEndpoint>;
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
    heartbeat_interval_secs: u64,
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
            .field("heartbeat_interval_secs", &self.heartbeat_interval_secs)
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
            heartbeat_interval_secs: 240,
            service_token: None,
            service_token_file: None,
        }
    }

    /// Sets the heartbeat interval forwarded to every spawned banquette.
    ///
    /// The interval controls how often the room is persisted to cellar. The
    /// Windows package uses 60 seconds so a forced shutdown loses at most about
    /// one minute of room state.
    #[must_use]
    pub fn with_heartbeat_interval_secs(mut self, seconds: u64) -> Self {
        self.heartbeat_interval_secs = seconds;
        self
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

    /// Builds the argument list for a specific transport endpoint.
    ///
    /// Exposed so tests can assert the "no secret in argv" property and that no
    /// room head is passed: the room restores itself from cellar's
    /// `room/<room_id>/head` alias (Issue #192).
    pub fn command_args_for_endpoint(
        &self,
        room_id: u64,
        admin_account: u64,
        endpoint: &BanquetteEndpoint,
    ) -> Vec<String> {
        let mut args = vec![
            "--room-id".to_string(),
            format!("{room_id:016x}"),
            "--admin".to_string(),
            admin_account.to_string(),
        ];

        match endpoint {
            #[cfg(unix)]
            BanquetteEndpoint::UnixSocket(path) => {
                args.push("--socket".to_string());
                args.push(path.display().to_string());
            }
            #[cfg(windows)]
            BanquetteEndpoint::NamedPipe(name) => {
                args.push("--pipe-name".to_string());
                args.push(name.clone());
            }
        }

        args.extend([
            "--cellar-url".to_string(),
            self.cellar_url.clone(),
            "--chunk-size-exponent".to_string(),
            self.chunk_size_exponent.to_string(),
            "--heartbeat-interval-secs".to_string(),
            self.heartbeat_interval_secs.to_string(),
        ]);

        if let Some(path) = self.service_token_file.as_deref() {
            args.push("--service-token-file".to_string());
            args.push(path.display().to_string());
        }

        args
    }

    /// Builds the argument list for a UNIX socket endpoint.
    #[cfg(unix)]
    pub fn command_args(
        &self,
        room_id: u64,
        admin_account: u64,
        socket_path: &std::path::Path,
    ) -> Vec<String> {
        self.command_args_for_endpoint(
            room_id,
            admin_account,
            &BanquetteEndpoint::UnixSocket(socket_path.to_path_buf()),
        )
    }

    /// Passes the service token to the child without putting it in argv.
    fn configure_env(&self, cmd: &mut tokio::process::Command) {
        if self.service_token_file.is_none()
            && let Some(token) = self.service_token.as_deref()
        {
            cmd.env("ALHALTE_SERVICE_TOKEN", token);
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
/// On Windows the resolved name gets the `.exe` suffix when it does not already
/// have one.
pub fn resolve_banquette_bin(binary_path: &std::path::Path) -> PathBuf {
    if binary_path.is_file() {
        return binary_path.to_path_buf();
    }

    let mut bin_name = binary_path
        .file_name()
        .unwrap_or(binary_path.as_os_str())
        .to_os_string();
    let suffix = std::env::consts::EXE_SUFFIX;
    if !suffix.is_empty()
        && !bin_name
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(suffix)
    {
        bin_name.push(suffix);
    }

    // 1. Current working directory target/debug and target/release
    let candidates = [
        PathBuf::from("target/debug").join(&bin_name),
        PathBuf::from("target/release").join(&bin_name),
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
        let candidate = parent.join(&bin_name);
        if candidate.is_file() {
            return candidate;
        }
    }

    // 3. Search in PATH
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(&bin_name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    binary_path.to_path_buf()
}

#[async_trait]
impl BanquetteSpawner for ProcessBanquetteSpawner {
    async fn spawn_banquette(&self, room_id: u64, admin_account: u64) -> Result<BanquetteEndpoint> {
        let resolved_bin = resolve_banquette_bin(&self.binary_path);
        tracing::debug!(
            "Spawning banquette for room {room_id} with binary: {:?}",
            resolved_bin
        );

        #[cfg(unix)]
        {
            tokio::fs::create_dir_all(&self.socket_dir)
                .await
                .map_err(MaitreDError::Io)?;

            let socket_path = self.socket_dir.join(format!("banquette-{room_id}.sock"));
            if socket_path.exists() {
                let _ = tokio::fs::remove_file(&socket_path).await;
            }

            let endpoint = BanquetteEndpoint::UnixSocket(socket_path);
            let mut cmd = tokio::process::Command::new(&resolved_bin);
            cmd.args(self.command_args_for_endpoint(room_id, admin_account, &endpoint));
            self.configure_env(&mut cmd);
            cmd.spawn().map_err(|e| {
                MaitreDError::Banquette(format!(
                    "Failed to spawn banquette binary at {:?} (resolved: {:?}): {e}",
                    self.binary_path, resolved_bin
                ))
            })?;
            Ok(endpoint)
        }

        #[cfg(windows)]
        {
            let pipe_name = format!(
                r"\\.\pipe\alhalte-banquette-{room_id:016x}-{:016x}",
                rand::random::<u64>()
            );
            let endpoint = BanquetteEndpoint::NamedPipe(pipe_name);
            let mut cmd = tokio::process::Command::new(&resolved_bin);
            cmd.args(self.command_args_for_endpoint(room_id, admin_account, &endpoint));
            self.configure_env(&mut cmd);
            cmd.spawn().map_err(|e| {
                MaitreDError::Banquette(format!(
                    "Failed to spawn banquette binary at {:?} (resolved: {:?}): {e}",
                    self.binary_path, resolved_bin
                ))
            })?;
            Ok(endpoint)
        }
    }
}

/// A running in-process room server together with its graceful shutdown handle.
#[cfg(unix)]
struct InProcessRoom {
    shutdown: watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
}

/// Spawns in-process [`BanquetteServer`] instances for automated testing without child processes.
#[cfg(unix)]
#[derive(Clone)]
pub struct InProcessBanquetteSpawner {
    socket_dir: PathBuf,
    cellar: Arc<dyn CellarSink>,
    chunk_size_exponent: u8,
    heartbeat_interval: Duration,
    rooms: Arc<Mutex<HashMap<u64, InProcessRoom>>>,
}

#[cfg(unix)]
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

#[cfg(unix)]
#[async_trait]
impl BanquetteSpawner for InProcessBanquetteSpawner {
    async fn spawn_banquette(&self, room_id: u64, admin_account: u64) -> Result<BanquetteEndpoint> {
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

        // The room restores itself from cellar's `room/<room_id>/head` alias, or
        // starts new when the alias is unset. A broken alias is an error, never an
        // empty room.
        let room = Room::load_or_create(
            self.cellar.as_ref(),
            room_id,
            admin_account,
            self.chunk_size_exponent,
        )
        .await
        .map_err(|e| MaitreDError::Banquette(format!("failed to start room {room_id}: {e}")))?;

        let config = ServerConfig {
            socket_path: socket_path.clone(),
            pipe_name: None,
            heartbeat_interval: self.heartbeat_interval,
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

        Ok(BanquetteEndpoint::UnixSocket(socket_path))
    }
}
