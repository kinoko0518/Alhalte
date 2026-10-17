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

/// Core service encapsulating cellar logic.
pub struct CellarService {
    storage: StorageManager,
    db: Arc<Mutex<Connection>>,
    verifier: Arc<dyn SessionVerifier>,
    anti_oracle: bool,
    gc_interval: Option<Duration>,
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

    /// Performs the initial store metadata handshake.
    ///
    /// Evaluates storage rights and checks whether upload is needed:
    /// - Returns `210` (UploadRequired) if file must be uploaded or if `anti_oracle` is enabled.
    /// - Returns `211` (Deduplicated) if already stored by another owner (adding a DB reference row).
    /// - Returns `212` (AlreadyOwned) if already owned by this owner.
    pub async fn store_check(&self, req: &StoreRequest) -> Result<StoreStatus> {
        let is_valid = self
            .verifier
            .verify_session(req.owner, req.session_id)
            .await?;
        if !is_valid {
            return Err(CellarError::Unauthorized {
                owner: req.owner,
                session_id: req.session_id,
            });
        }

        let conn = self.db.lock().await;
        let already_owned = Database::has_owner_reference(&conn, &req.tin_id, req.owner)?;
        let file_exists = self.storage.has_tin_file(&req.tin_id);

        if self.anti_oracle {
            // Anti-oracle mode: always return 210 to avoid revealing existence.
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
        let is_valid = self.verifier.verify_session(owner, session_id).await?;
        if !is_valid {
            return Err(CellarError::Unauthorized { owner, session_id });
        }

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
        let is_valid = self.verifier.verify_session(owner, session_id).await?;
        if !is_valid {
            return Err(CellarError::Unauthorized { owner, session_id });
        }

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
