//! Abstraction and implementations for persisting tin blobs to `alhalte-cellar`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::StatusCode;
use tokio::sync::RwLock;

use alhalte_cellar::{
    StoreRequest, StoreResponse, StoreResult, StoreStatus, UnregisterRequest, format_tin_id,
};
use tin::TinId;

use crate::error::{BanquetteError, Result};

/// Session ID used for internal room-level writes.
///
/// Room state persistence is authorized by the cellar service token
/// (`X-Alhalte-Service-Token`), never by a synthetic user session, so no real
/// session is sent. Client-session writes (log chunks, scene unloads) still use
/// the real session of the client that triggered them.
pub const INTERNAL_WRITE_SESSION_ID: u64 = 0;

/// Storage abstraction for sending sealed chunks and persisted scene/room tins to cellar.
#[async_trait]
pub trait CellarSink: Send + Sync {
    /// Stores a raw encoded tin binary in the cellar storage service.
    async fn store_tin(
        &self,
        owner: u64,
        session_id: u64,
        tin_bytes: &[u8],
        tin_id: &TinId,
        wrapped_key: Option<Vec<u8>>,
    ) -> Result<StoreResult>;

    /// Downloads a raw tin binary by its 256-bit `tin_id`.
    async fn download_tin(&self, tin_id: &TinId, owner: Option<u64>) -> Result<Vec<u8>>;

    /// Removes the `(owner, tin_id)` reference so cellar GC can reclaim the entity.
    ///
    /// Returns `true` when a reference row was deleted and `false` when the
    /// reference did not exist (already reclaimed). Retention of superseded room
    /// heads is best effort: callers log and continue on errors.
    async fn unregister_tin(&self, owner: u64, session_id: u64, tin_id: &TinId) -> Result<bool>;
}

#[derive(Debug, Clone)]
struct StoredTin {
    data: Vec<u8>,
    wrapped_key: Option<Vec<u8>>,
}

/// In-memory implementation of [`CellarSink`] useful for unit tests and local operation.
#[derive(Debug, Default, Clone)]
pub struct InMemoryCellarSink {
    storage: Arc<RwLock<HashMap<TinId, StoredTin>>>,
}

impl InMemoryCellarSink {
    /// Creates a new empty [`InMemoryCellarSink`].
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Checks if a tin exists in the in-memory storage.
    pub async fn contains(&self, tin_id: &TinId) -> bool {
        let read = self.storage.read().await;
        read.contains_key(tin_id)
    }

    /// Downloads a stored tin and its optional wrapped key from in-memory storage.
    pub async fn download_with_key(&self, tin_id: &TinId) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
        let read = self.storage.read().await;
        match read.get(tin_id) {
            Some(stored) => Ok((stored.data.clone(), stored.wrapped_key.clone())),
            None => Err(BanquetteError::Cellar(format!(
                "Tin {} not found in memory sink",
                format_tin_id(tin_id)
            ))),
        }
    }
}

#[async_trait]
impl CellarSink for InMemoryCellarSink {
    async fn store_tin(
        &self,
        owner: u64,
        _session_id: u64,
        tin_bytes: &[u8],
        tin_id: &TinId,
        wrapped_key: Option<Vec<u8>>,
    ) -> Result<StoreResult> {
        let mut write = self.storage.write().await;
        let reference_added = !write.contains_key(tin_id);
        write.insert(
            *tin_id,
            StoredTin {
                data: tin_bytes.to_vec(),
                wrapped_key,
            },
        );

        Ok(StoreResult {
            tin_id: *tin_id,
            owner,
            written_to_disk: true,
            reference_added,
        })
    }

    async fn download_tin(&self, tin_id: &TinId, _owner: Option<u64>) -> Result<Vec<u8>> {
        let read = self.storage.read().await;
        match read.get(tin_id) {
            Some(stored) => Ok(stored.data.clone()),
            None => Err(BanquetteError::Cellar(format!(
                "Tin {} not found in memory sink",
                format_tin_id(tin_id)
            ))),
        }
    }

    async fn unregister_tin(&self, _owner: u64, _session_id: u64, tin_id: &TinId) -> Result<bool> {
        // The in-memory sink models a single owner per tin, so dropping the
        // stored entry is equivalent to removing the last reference row.
        let mut write = self.storage.write().await;
        Ok(write.remove(tin_id).is_some())
    }
}

/// HTTP client implementation of [`CellarSink`] communicating with a remote `alhalte-cellar` server.
///
/// When a cellar service token is configured, internal writes (room state
/// persistence and superseded-head retention) carry it in the
/// `X-Alhalte-Service-Token` header instead of a synthetic user session. The
/// token is redacted from the [`std::fmt::Debug`] representation.
#[derive(Clone)]
pub struct HttpCellarSink {
    base_url: String,
    client: reqwest::Client,
    service_token: Option<String>,
}

impl std::fmt::Debug for HttpCellarSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpCellarSink")
            .field("base_url", &self.base_url)
            .field(
                "service_token",
                &self.service_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl HttpCellarSink {
    /// Creates a new [`HttpCellarSink`] with the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            service_token: None,
        }
    }

    /// Configures the internal cellar service token sent with internal writes.
    ///
    /// An empty token is treated as absent. The value is never logged.
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

    /// Returns whether a service token is configured.
    #[must_use]
    pub fn has_service_token(&self) -> bool {
        self.service_token.is_some()
    }

    /// Adds the service token header to an internal write request, if configured.
    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.service_token.as_deref() {
            Some(token) => request.header(alhalte_cellar::handlers::SERVICE_TOKEN_HEADER, token),
            None => request,
        }
    }
}

#[async_trait]
impl CellarSink for HttpCellarSink {
    async fn store_tin(
        &self,
        owner: u64,
        session_id: u64,
        tin_bytes: &[u8],
        tin_id: &TinId,
        wrapped_key: Option<Vec<u8>>,
    ) -> Result<StoreResult> {
        let check_url = format!("{}/store", self.base_url);
        let store_req = StoreRequest {
            owner,
            session_id,
            tin_id: *tin_id,
            size: tin_bytes.len() as u64,
            wrapped_key: wrapped_key.clone(),
        };

        let resp = self
            .authorize(self.client.post(&check_url).json(&store_req))
            .send()
            .await
            .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

        // The store check always answers `200 OK` with `{"result": ...}`.
        // Every other status (401 / 400 / 5xx, or a legacy 210 / 211 / 212) is
        // an error; the old non-standard status codes are not accepted.
        if resp.status() != StatusCode::OK {
            return Err(BanquetteError::Cellar(format!(
                "Cellar store check failed with HTTP status {}",
                resp.status()
            )));
        }

        let check: StoreResponse = resp
            .json()
            .await
            .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

        match check.result {
            StoreStatus::UploadRequired => {
                // Need to upload the body
                let upload_url = format!("{}/store/{}", self.base_url, format_tin_id(tin_id));
                let mut req = self
                    .client
                    .post(&upload_url)
                    .header("X-Alhalte-Owner", owner.to_string())
                    .header("X-Alhalte-Session-Id", session_id.to_string())
                    .body(Bytes::copy_from_slice(tin_bytes));

                if let Some(ref key) = wrapped_key {
                    req = req.header("X-Alhalte-Wrapped-Key", hex::encode(key));
                }

                let upload_resp = self
                    .authorize(req)
                    .send()
                    .await
                    .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

                if !upload_resp.status().is_success() {
                    return Err(BanquetteError::Cellar(format!(
                        "Cellar upload failed with HTTP status {}",
                        upload_resp.status()
                    )));
                }

                let result: StoreResult = upload_resp
                    .json()
                    .await
                    .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

                Ok(result)
            }
            StoreStatus::Deduplicated => Ok(StoreResult {
                tin_id: *tin_id,
                owner,
                written_to_disk: false,
                reference_added: true,
            }),
            StoreStatus::AlreadyOwned => Ok(StoreResult {
                tin_id: *tin_id,
                owner,
                written_to_disk: false,
                reference_added: false,
            }),
        }
    }

    async fn download_tin(&self, tin_id: &TinId, owner: Option<u64>) -> Result<Vec<u8>> {
        let download_url = format!("{}/tin/{}", self.base_url, format_tin_id(tin_id));
        let mut req = self.client.get(&download_url);
        if let Some(o) = owner {
            req = req.header("X-Alhalte-Owner", o.to_string());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Err(BanquetteError::Cellar(format!(
                "Tin {} not found in cellar",
                format_tin_id(tin_id)
            )));
        }

        if !resp.status().is_success() {
            return Err(BanquetteError::Cellar(format!(
                "Cellar download failed with status {}",
                resp.status()
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

        Ok(bytes.to_vec())
    }

    async fn unregister_tin(&self, owner: u64, session_id: u64, tin_id: &TinId) -> Result<bool> {
        let unregister_url = format!("{}/unregister", self.base_url);
        let request = UnregisterRequest {
            owner,
            session_id,
            tin_id: *tin_id,
        };

        let resp = self
            .authorize(self.client.post(&unregister_url).json(&request))
            .send()
            .await
            .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !resp.status().is_success() {
            return Err(BanquetteError::Cellar(format!(
                "Cellar unregister failed with status {}",
                resp.status()
            )));
        }

        Ok(true)
    }
}
