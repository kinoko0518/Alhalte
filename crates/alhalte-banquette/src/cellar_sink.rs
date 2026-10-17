//! Abstraction and implementations for persisting tin blobs to `alhalte-cellar`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::StatusCode;
use tokio::sync::RwLock;

use alhalte_cellar::{StoreRequest, StoreResult, StoreStatus, format_tin_id};
use tin::TinId;

use crate::error::{BanquetteError, Result};

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
}

/// HTTP client implementation of [`CellarSink`] communicating with a remote `alhalte-cellar` server.
#[derive(Debug, Clone)]
pub struct HttpCellarSink {
    base_url: String,
    client: reqwest::Client,
}

impl HttpCellarSink {
    /// Creates a new [`HttpCellarSink`] with the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
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
            .client
            .post(&check_url)
            .json(&store_req)
            .send()
            .await
            .map_err(|e| BanquetteError::Cellar(e.to_string()))?;

        let status = resp.status();
        let status_code = status.as_u16();

        if status_code == StoreStatus::UploadRequired.as_u16() {
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

            let upload_resp = req
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
        } else if status_code == StoreStatus::Deduplicated.as_u16() {
            Ok(StoreResult {
                tin_id: *tin_id,
                owner,
                written_to_disk: false,
                reference_added: true,
            })
        } else if status_code == StoreStatus::AlreadyOwned.as_u16() {
            Ok(StoreResult {
                tin_id: *tin_id,
                owner,
                written_to_disk: false,
                reference_added: false,
            })
        } else {
            Err(BanquetteError::Cellar(format!(
                "Unexpected response status from cellar store check: {status}"
            )))
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
}
