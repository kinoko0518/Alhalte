//! High-level cellar service orchestrating database, storage, verifier, and GC.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use rusqlite::Connection;
use tin::TinId;
use tokio::sync::Mutex;

use crate::db::Database;
use crate::error::{CellarError, Result};
use crate::gc::run_garbage_collection;
use crate::handlers::create_router;
use crate::model::{GcStats, StoreRequest, StoreResult, StoreStatus};
use crate::storage::StorageManager;
use crate::verifier::SessionVerifier;

/// Configuration options for initializing `CellarService`.
#[derive(Debug, Clone)]
pub struct CellarConfig {
    /// Host interface to bind to (e.g., `"127.0.0.1"`).
    pub host: String,
    /// Port to listen on (e.g., `8081`).
    pub port: u16,
    /// Directory where tin files are stored.
    pub storage_path: PathBuf,
    /// Path to the SQLite database file.
    pub db_path: PathBuf,
    /// Whether anti-oracle behavior is enabled.
    pub anti_oracle: bool,
    /// Optional periodic GC execution interval.
    pub gc_interval: Option<Duration>,
}

impl Default for CellarConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8081,
            storage_path: PathBuf::from("./data/cellar/tin"),
            db_path: PathBuf::from("./data/cellar/cellar.sqlite3"),
            anti_oracle: false,
            gc_interval: Some(Duration::from_secs(3600)),
        }
    }
}

/// Compares two byte strings without an early exit on the first mismatch.
///
/// Used for the internal service token so that response timing does not leak
/// how many prefix bytes were correct.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Core service encapsulating cellar logic.
pub struct CellarService {
    storage: StorageManager,
    db: Arc<Mutex<Connection>>,
    verifier: Arc<dyn SessionVerifier>,
    anti_oracle: bool,
    gc_interval: Option<Duration>,
    service_token: Option<String>,
}

impl CellarService {
    /// Creates a new `CellarService` from configuration and a session verifier.
    pub fn new(config: &CellarConfig, verifier: Arc<dyn SessionVerifier>) -> Result<Self> {
        let storage = StorageManager::new(&config.storage_path)?;
        let conn = Database::open(&config.db_path)?;

        Ok(Self {
            storage,
            db: Arc::new(Mutex::new(conn)),
            verifier,
            anti_oracle: config.anti_oracle,
            gc_interval: config.gc_interval,
            service_token: None,
        })
    }

    /// Creates a `CellarService` using an in-memory database and specified storage directory (primarily for testing).
    pub fn new_with_in_memory_db(
        storage_path: impl Into<PathBuf>,
        verifier: Arc<dyn SessionVerifier>,
        anti_oracle: bool,
    ) -> Result<Self> {
        let storage = StorageManager::new(storage_path)?;
        let conn = Database::open_in_memory()?;

        Ok(Self {
            storage,
            db: Arc::new(Mutex::new(conn)),
            verifier,
            anti_oracle,
            gc_interval: None,
            service_token: None,
        })
    }

    /// Returns a reference to the underlying storage manager.
    #[must_use]
    pub fn storage(&self) -> &StorageManager {
        &self.storage
    }

    /// Returns whether anti-oracle mode is active.
    #[must_use]
    pub fn anti_oracle(&self) -> bool {
        self.anti_oracle
    }

    /// Returns a reference to the session verifier.
    #[must_use]
    pub fn verifier(&self) -> &Arc<dyn SessionVerifier> {
        &self.verifier
    }

    /// Configures the internal service token used for trusted service-to-service writes.
    ///
    /// When set, a store check / upload / unregister request may present the
    /// token in the `X-Alhalte-Service-Token` header instead of a user session.
    /// This is how `alhalte-banquette` persists room state without impersonating
    /// a synthetic user session. User session verification is unchanged: requests
    /// without the header still go through [`SessionVerifier`].
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

    /// Returns whether an internal service token is configured.
    #[must_use]
    pub fn has_service_token(&self) -> bool {
        self.service_token.is_some()
    }

    /// Authorizes an internal write.
    ///
    /// A request that carries a `service_token` is authorized only by that token:
    /// it never falls back to session verification, so a wrong token is rejected
    /// rather than silently accepted through a user session.
    async fn authorize(
        &self,
        owner: u64,
        session_id: u64,
        service_token: Option<&str>,
    ) -> Result<()> {
        let Some(presented) = service_token else {
            let is_valid = self.verifier.verify_session(owner, session_id).await?;
            return if is_valid {
                Ok(())
            } else {
                Err(CellarError::Unauthorized { owner, session_id })
            };
        };

        match self.service_token.as_deref() {
            Some(expected) if constant_time_eq(presented.as_bytes(), expected.as_bytes()) => Ok(()),
            // No token configured, or a mismatch: fail closed with the same error
            // as an invalid session so callers cannot probe the token.
            _ => Err(CellarError::Unauthorized { owner, session_id }),
        }
    }

    /// Performs the initial store metadata handshake.
    ///
    /// Evaluates storage rights and checks whether upload is needed:
    /// - Returns [`StoreStatus::UploadRequired`] if the file must be uploaded or
    ///   if `anti_oracle` is enabled.
    /// - Returns [`StoreStatus::Deduplicated`] if already stored by another owner
    ///   (adding a DB reference row).
    /// - Returns [`StoreStatus::AlreadyOwned`] if already owned by this owner.
    pub async fn store_check(&self, req: &StoreRequest) -> Result<StoreStatus> {
        self.store_check_with_token(req, None).await
    }

    /// Performs the store metadata handshake with an optional internal service token.
    ///
    /// See [`Self::store_check`] and [`Self::with_service_token`].
    pub async fn store_check_with_token(
        &self,
        req: &StoreRequest,
        service_token: Option<&str>,
    ) -> Result<StoreStatus> {
        self.authorize(req.owner, req.session_id, service_token)
            .await?;

        let conn = self.db.lock().await;
        let already_owned = Database::has_owner_reference(&conn, &req.tin_id, req.owner)?;
        let file_exists = self.storage.has_tin_file(&req.tin_id);

        if self.anti_oracle {
            // Anti-oracle mode: always report upload_required to avoid revealing existence.
            Ok(StoreStatus::UploadRequired)
        } else if already_owned {
            // Already owned by this owner
            Ok(StoreStatus::AlreadyOwned)
        } else if file_exists {
            // Deduplicated: file exists in storage from another owner, add reference row
            let relative_path = StorageManager::relative_name(&req.tin_id);
            Database::insert_reference(
                &conn,
                &req.tin_id,
                &relative_path,
                req.owner,
                req.wrapped_key.as_deref(),
            )?;
            Ok(StoreStatus::Deduplicated)
        } else {
            // File does not exist yet; client needs to upload it
            Ok(StoreStatus::UploadRequired)
        }
    }

    /// Stores an uploaded tin payload.
    ///
    /// Verifies session authorization, saves to disk (or discards to `/dev/null` if anti-oracle
    /// and entity already exists), and records the ownership row in the SQLite database.
    pub async fn store_payload(
        &self,
        owner: u64,
        session_id: u64,
        tin_id: TinId,
        data: &[u8],
        wrapped_key: Option<Vec<u8>>,
    ) -> Result<StoreResult> {
        self.store_payload_with_token(owner, session_id, tin_id, data, wrapped_key, None)
            .await
    }

    /// Stores an uploaded tin payload with an optional internal service token.
    ///
    /// See [`Self::store_payload`] and [`Self::with_service_token`].
    pub async fn store_payload_with_token(
        &self,
        owner: u64,
        session_id: u64,
        tin_id: TinId,
        data: &[u8],
        wrapped_key: Option<Vec<u8>>,
        service_token: Option<&str>,
    ) -> Result<StoreResult> {
        self.authorize(owner, session_id, service_token).await?;

        let file_exists = self.storage.has_tin_file(&tin_id);
        let written_to_disk = if self.anti_oracle && file_exists {
            // Anti-oracle mode: file already exists, discard uploaded data to /dev/null
            StorageManager::discard_to_dev_null(data);
            false
        } else if !file_exists {
            // File does not exist, write to disk
            self.storage.write_tin(&tin_id, data)?;
            true
        } else {
            // Not anti-oracle, but file already exists on disk (e.g. concurrent upload)
            false
        };

        let conn = self.db.lock().await;
        let relative_path = StorageManager::relative_name(&tin_id);
        Database::insert_reference(
            &conn,
            &tin_id,
            &relative_path,
            owner,
            wrapped_key.as_deref(),
        )?;

        Ok(StoreResult {
            tin_id,
            owner,
            written_to_disk,
            reference_added: true,
        })
    }

    /// Downloads a stored tin by its `tin_id`, optionally filtered by `owner`.
    ///
    /// Returns the raw tin binary along with its optional `wrapped_key`.
    pub async fn download(
        &self,
        tin_id: &TinId,
        owner: Option<u64>,
    ) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
        let conn = self.db.lock().await;
        let reference = Database::get_reference(&conn, tin_id, owner)?;
        let reference = match reference {
            Some(r) => r,
            None => {
                return Err(CellarError::NotFound(crate::model::format_tin_id(tin_id)));
            }
        };

        let data = self.storage.read_tin(tin_id)?;
        Ok((data, reference.wrapped_key))
    }

    /// Removes an ownership reference for `(tin_id, owner)` after verifying the session.
    ///
    /// Returns `true` if a row was deleted, `false` if not found.
    pub async fn unregister(&self, owner: u64, session_id: u64, tin_id: &TinId) -> Result<bool> {
        self.unregister_with_token(owner, session_id, tin_id, None)
            .await
    }

    /// Removes an ownership reference with an optional internal service token.
    ///
    /// See [`Self::unregister`] and [`Self::with_service_token`].
    pub async fn unregister_with_token(
        &self,
        owner: u64,
        session_id: u64,
        tin_id: &TinId,
        service_token: Option<&str>,
    ) -> Result<bool> {
        self.authorize(owner, session_id, service_token).await?;

        let conn = self.db.lock().await;
        Database::delete_reference(&conn, tin_id, owner)
    }

    /// Runs garbage collection over the storage directory.
    pub async fn run_gc(&self) -> Result<GcStats> {
        let conn = self.db.lock().await;
        run_garbage_collection(&self.storage, &conn)
    }

    /// Spawns a background task that periodically runs garbage collection.
    pub fn spawn_gc_task(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        let interval = self.gc_interval?;
        let this = Arc::clone(self);

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // Initial tick
            loop {
                ticker.tick().await;
                if let Err(e) = this.run_gc().await {
                    tracing::error!("Periodic GC failed: {e}");
                }
            }
        });

        Some(handle)
    }

    /// Constructs the Axum `Router` serving the cellar HTTP API.
    pub fn into_router(self: Arc<Self>) -> Router {
        create_router(self)
    }
}
