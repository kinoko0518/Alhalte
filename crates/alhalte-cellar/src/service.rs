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
use crate::model::{
    GcStats, SYSTEM_OWNER, StoreRequest, StoreResult, StoreStatus, WriteCredentials,
};
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

/// Builds the 401 error for incomplete or rejected header credentials.
///
/// The reported pair is the claim (or the [`SYSTEM_OWNER`] / `0` placeholder
/// when a header was absent) so the response shape stays stable.
fn unauthorized(credentials: WriteCredentials) -> CellarError {
    CellarError::Unauthorized {
        owner: credentials.owner.unwrap_or(SYSTEM_OWNER),
        session_id: credentials.session_id.unwrap_or(0),
    }
}

/// Credential that authorized a cellar write.
///
/// The credential, not the requested `owner`, decides the reference label:
/// user sessions keep their account ID, while the internal service token always
/// uses [`SYSTEM_OWNER`] so no request can pick the label of an internal write.
/// `owner` is never an authorization boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteAuthorization {
    /// Write authorized by a user session, labelled with this account ID.
    Session(u64),
    /// Write authorized by the internal service token.
    ServiceToken,
}

impl WriteAuthorization {
    /// Returns the reference label to persist for the authorized request.
    fn reference_owner(self) -> u64 {
        match self {
            Self::Session(owner) => owner,
            Self::ServiceToken => SYSTEM_OWNER,
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

    /// Authorizes a write and reports which credential authorized it.
    ///
    /// A request that carries a `service_token` is authorized only by that token:
    /// it never falls back to session verification, so a wrong token is rejected
    /// rather than silently accepted through a user session, and any claimed
    /// `owner` is replaced by [`SYSTEM_OWNER`].
    ///
    /// Without a token both header credentials are required, because
    /// [`SessionVerifier`] binds the reference label to the session; a missing or
    /// mismatching pair is rejected with 401.
    async fn authorize(
        &self,
        credentials: WriteCredentials,
        service_token: Option<&str>,
    ) -> Result<WriteAuthorization> {
        if let Some(presented) = service_token {
            return match self.service_token.as_deref() {
                Some(expected) if constant_time_eq(presented.as_bytes(), expected.as_bytes()) => {
                    Ok(WriteAuthorization::ServiceToken)
                }
                // No token configured, or a mismatch: fail closed with the same
                // error as an invalid session so callers cannot probe the token.
                _ => Err(unauthorized(credentials)),
            };
        }

        let (Some(owner), Some(session_id)) = (credentials.owner, credentials.session_id) else {
            return Err(unauthorized(credentials));
        };

        let is_valid = self.verifier.verify_session(owner, session_id).await?;
        if is_valid {
            Ok(WriteAuthorization::Session(owner))
        } else {
            Err(CellarError::Unauthorized { owner, session_id })
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
    ///
    /// `credentials` comes from the request headers (see [`WriteCredentials`]);
    /// the body never carries an ownership claim.
    pub async fn store_check(
        &self,
        req: &StoreRequest,
        credentials: WriteCredentials,
    ) -> Result<StoreStatus> {
        self.store_check_with_token(req, credentials, None).await
    }

    /// Performs the store metadata handshake with an optional internal service token.
    ///
    /// The reference label is the claimed `owner` for a user session, and
    /// [`SYSTEM_OWNER`] when the internal service token authorized the request.
    /// `owner` never grants access by itself.
    ///
    /// See [`Self::store_check`] and [`Self::with_service_token`].
    pub async fn store_check_with_token(
        &self,
        req: &StoreRequest,
        credentials: WriteCredentials,
        service_token: Option<&str>,
    ) -> Result<StoreStatus> {
        let authorization = self.authorize(credentials, service_token).await?;
        let owner = authorization.reference_owner();

        let conn = self.db.lock().await;
        let already_owned = Database::has_owner_reference(&conn, &req.tin_id, owner)?;
        let file_exists = self.storage.has_tin_file(&req.tin_id);

        if self.anti_oracle {
            // Anti-oracle mode: always report upload_required to avoid revealing existence.
            Ok(StoreStatus::UploadRequired)
        } else if already_owned {
            // A reference with this label already exists
            Ok(StoreStatus::AlreadyOwned)
        } else if file_exists {
            // Deduplicated: file exists in storage under another label, add a reference row
            Database::insert_reference(&conn, &req.tin_id, owner, req.wrapped_key.as_deref())?;
            Ok(StoreStatus::Deduplicated)
        } else {
            // File does not exist yet; client needs to upload it
            Ok(StoreStatus::UploadRequired)
        }
    }

    /// Stores an uploaded tin payload.
    ///
    /// Verifies session authorization, saves to disk (or discards the payload if anti-oracle
    /// and the entity already exists), and records the ownership row in the SQLite database.
    pub async fn store_payload(
        &self,
        tin_id: TinId,
        data: &[u8],
        wrapped_key: Option<Vec<u8>>,
        credentials: WriteCredentials,
    ) -> Result<StoreResult> {
        self.store_payload_with_token(tin_id, data, wrapped_key, credentials, None)
            .await
    }

    /// Stores an uploaded tin payload with an optional internal service token.
    ///
    /// The stored reference label is the claimed `owner` for a user session and
    /// [`SYSTEM_OWNER`] for a service-token write; [`StoreResult::owner`] reports
    /// the label that was actually written.
    ///
    /// See [`Self::store_payload`] and [`Self::with_service_token`].
    pub async fn store_payload_with_token(
        &self,
        tin_id: TinId,
        data: &[u8],
        wrapped_key: Option<Vec<u8>>,
        credentials: WriteCredentials,
        service_token: Option<&str>,
    ) -> Result<StoreResult> {
        let authorization = self.authorize(credentials, service_token).await?;
        let owner = authorization.reference_owner();

        let file_exists = self.storage.has_tin_file(&tin_id);
        let written_to_disk = if self.anti_oracle && file_exists {
            // Anti-oracle mode: the entity already exists, so drop the uploaded bytes
            StorageManager::discard(data);
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
        Database::insert_reference(&conn, &tin_id, owner, wrapped_key.as_deref())?;

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
    pub async fn unregister(&self, tin_id: &TinId, credentials: WriteCredentials) -> Result<bool> {
        self.unregister_with_token(tin_id, credentials, None).await
    }

    /// Removes a reference with an optional internal service token.
    ///
    /// A service-token unregister removes the [`SYSTEM_OWNER`] reference
    /// regardless of the claimed owner, mirroring service-token stores.
    ///
    /// See [`Self::unregister`] and [`Self::with_service_token`].
    pub async fn unregister_with_token(
        &self,
        tin_id: &TinId,
        credentials: WriteCredentials,
        service_token: Option<&str>,
    ) -> Result<bool> {
        let authorization = self.authorize(credentials, service_token).await?;
        let owner = authorization.reference_owner();

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
