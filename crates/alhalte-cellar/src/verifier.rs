//! Session verification abstraction for checking storage permissions with `alhalte-maitre-d`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{CellarError, Result};

/// Trait defining the contract for verifying that an owner (account ID) holds a valid session ID.
#[async_trait]
pub trait SessionVerifier: Send + Sync {
    /// Checks whether `(owner, session_id)` represents an active, authorized session.
    async fn verify_session(&self, owner: u64, session_id: u64) -> Result<bool>;
}

/// In-memory session verifier for unit and integration testing.
#[derive(Debug, Default)]
pub struct InMemorySessionVerifier {
    active_sessions: RwLock<HashSet<(u64, u64)>>,
    allow_all: RwLock<bool>,
}

impl InMemorySessionVerifier {
    /// Creates a new empty `InMemorySessionVerifier`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an `InMemorySessionVerifier` that allows all sessions unconditionally.
    #[must_use]
    pub fn allow_all_verifier() -> Self {
        Self {
            active_sessions: RwLock::new(HashSet::new()),
            allow_all: RwLock::new(true),
        }
    }

    /// Registers a valid `(owner, session_id)` pair.
    pub fn allow(&self, owner: u64, session_id: u64) {
        if let Ok(mut set) = self.active_sessions.write() {
            set.insert((owner, session_id));
        }
    }

    /// Revokes a `(owner, session_id)` pair.
    pub fn revoke(&self, owner: u64, session_id: u64) {
        if let Ok(mut set) = self.active_sessions.write() {
            set.remove(&(owner, session_id));
        }
    }

    /// Clears all allowed sessions.
    pub fn clear(&self) {
        if let Ok(mut set) = self.active_sessions.write() {
            set.clear();
        }
    }

    /// Sets whether to allow all queries unconditionally (useful for test setups).
    pub fn set_allow_all(&self, allow: bool) {
        if let Ok(mut flag) = self.allow_all.write() {
            *flag = allow;
        }
    }
}

#[async_trait]
impl SessionVerifier for InMemorySessionVerifier {
    async fn verify_session(&self, owner: u64, session_id: u64) -> Result<bool> {
        if let Ok(flag) = self.allow_all.read()
            && *flag
        {
            return Ok(true);
        }
        if let Ok(set) = self.active_sessions.read() {
            return Ok(set.contains(&(owner, session_id)));
        }
        Ok(false)
    }
}

/// Communication target for querying `alhalte-maitre-d`.
#[derive(Debug, Clone)]
pub enum MaitreDTarget {
    /// HTTP host and port (e.g., `"127.0.0.1:8080"`).
    Http(String),
    /// Unix domain socket path (e.g., `"/run/alhalte/maitre-d.sock"`).
    UnixSocket(PathBuf),
}

/// Verifier that queries the `alhalte-maitre-d` service via HTTP or Unix domain socket.
#[derive(Debug, Clone)]
pub struct MaitreDVerifier {
    target: MaitreDTarget,
}

impl MaitreDVerifier {
    /// Creates a verifier communicating over HTTP or TCP host:port.
    #[must_use]
    pub fn new_http(address: impl Into<String>) -> Self {
        let mut addr = address.into();
        if let Some(stripped) = addr.strip_prefix("http://") {
            addr = stripped.to_string();
        }
        // Remove trailing slash if present
        if let Some(stripped) = addr.strip_suffix('/') {
            addr = stripped.to_string();
        }
        Self {
            target: MaitreDTarget::Http(addr),
        }
    }

    /// Creates a verifier communicating over a Unix domain socket.
    #[must_use]
    pub fn new_unix(socket_path: impl AsRef<Path>) -> Self {
        Self {
            target: MaitreDTarget::UnixSocket(socket_path.as_ref().to_path_buf()),
        }
    }

    /// Sends the verification query over the configured transport and parses HTTP status.
    async fn send_query<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
        owner: u64,
        session_id: u64,
    ) -> Result<bool>
    where
        R: AsyncReadExt + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        let request = format!(
            "GET /api/verify-session?owner={owner}&session_id={session_id} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: close\r\n\
             \r\n"
        );

        writer.write_all(request.as_bytes()).await.map_err(|e| {
            CellarError::Verifier(format!("Failed to write request to maitre-d: {e}"))
        })?;
        writer.flush().await.map_err(|e| {
            CellarError::Verifier(format!("Failed to flush request to maitre-d: {e}"))
        })?;

        let mut response = Vec::with_capacity(512);
        let mut buf = [0u8; 256];
        loop {
            let n = reader.read(&mut buf).await.map_err(|e| {
                CellarError::Verifier(format!("Failed to read response from maitre-d: {e}"))
            })?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buf[..n]);
            // Stop early once we have received the HTTP header line
            if response.windows(2).any(|w| w == b"\r\n") {
                break;
            }
        }

        let resp_str = String::from_utf8_lossy(&response);
        let status_line = resp_str.lines().next().unwrap_or_default();

        // Check status line (e.g., "HTTP/1.1 200 OK")
        if status_line.contains(" 200") {
            Ok(true)
        } else if status_line.contains(" 401")
            || status_line.contains(" 403")
            || status_line.contains(" 404")
        {
            Ok(false)
        } else {
            Err(CellarError::Verifier(format!(
                "Unexpected response status from maitre-d: {status_line}"
            )))
        }
    }
}

#[async_trait]
impl SessionVerifier for MaitreDVerifier {
    async fn verify_session(&self, owner: u64, session_id: u64) -> Result<bool> {
        match &self.target {
            MaitreDTarget::Http(addr) => {
                let stream = tokio::net::TcpStream::connect(addr).await.map_err(|e| {
                    CellarError::Verifier(format!("Failed to connect to maitre-d at {addr}: {e}"))
                })?;
                let (reader, writer) = stream.into_split();
                self.send_query(reader, writer, owner, session_id).await
            }
            MaitreDTarget::UnixSocket(path) => {
                let stream = tokio::net::UnixStream::connect(path).await.map_err(|e| {
                    CellarError::Verifier(format!(
                        "Failed to connect to maitre-d unix socket at {}: {e}",
                        path.display()
                    ))
                })?;
                let (reader, writer) = stream.into_split();
                self.send_query(reader, writer, owner, session_id).await
            }
        }
    }
}
