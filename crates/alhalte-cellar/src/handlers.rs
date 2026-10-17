//! Axum HTTP route handlers and router configuration for `alhalte-cellar`.

use std::sync::Arc;

use axum::extract::{Path as AxPath, Query, State};
use axum::http::header::{CONTENT_TYPE, HeaderMap};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bytes::Bytes;
use serde::Deserialize;

use crate::error::CellarError;
use crate::model::{StoreRequest, StoreResponse, UnregisterRequest, format_tin_id, parse_tin_id};
use crate::service::CellarService;

/// Query parameters for download requests.
#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// Optional owner filter.
    pub owner: Option<u64>,
}

/// Query parameters for upload requests (fallback if headers not used).
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// Owner account ID.
    pub owner: Option<u64>,
    /// Session ID.
    pub session_id: Option<u64>,
    /// Optional wrapped key in hex.
    pub wrapped_key: Option<String>,
}

/// Helper to convert `CellarError` into an appropriate HTTP response.
impl IntoResponse for CellarError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::Unauthorized { .. } => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Self::AlreadyOwned { .. } => (StatusCode::CONFLICT, self.to_string()),
            Self::Hex(_) | Self::Json(_) | Self::SizeMismatch { .. } => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            Self::Io(_)
            | Self::Database(_)
            | Self::Tin(_)
            | Self::Verifier(_)
            | Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = serde_json::json!({
            "error": msg,
            "status": status.as_u16(),
        });

        (status, Json(body)).into_response()
    }
}

/// Builds the Axum router for the cellar service.
pub fn create_router(service: Arc<CellarService>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/store", post(handle_store_check))
        .route(
            "/store/{tin_id}",
            put(handle_store_upload)
                .post(handle_store_upload)
                .delete(handle_unregister_delete),
        )
        .route(
            "/tin/{tin_id}",
            put(handle_store_upload)
                .post(handle_store_upload)
                .get(handle_download)
                .delete(handle_unregister_delete),
        )
        .route("/tin/{tin_id}/meta", get(handle_tin_metadata))
        .route("/download/{tin_id}", get(handle_download))
        .route("/download/{tin_id}/meta", get(handle_tin_metadata))
        .route("/unregister", post(handle_unregister))
        .route("/gc", post(handle_gc))
        .with_state(service)
}

/// Healthcheck endpoint.
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

/// Store metadata check handler.
///
/// Returns 210 (UploadRequired), 211 (Deduplicated), or 212 (AlreadyOwned).
async fn handle_store_check(
    State(service): State<Arc<CellarService>>,
    Json(payload): Json<StoreRequest>,
) -> std::result::Result<Response, CellarError> {
    let tin_id = payload.tin_id;
    let status = service.store_check(&payload).await?;

    let response_body = StoreResponse {
        code: status.as_u16(),
        tin_id,
        message: status.message().to_string(),
    };

    let http_status = status.to_status_code();
    Ok((http_status, Json(response_body)).into_response())
}

/// Tin payload upload handler.
async fn handle_store_upload(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<UploadQuery>,
    body: Bytes,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    // Extract owner from header or query param
    let owner = headers
        .get("X-Alhalte-Owner")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query.owner)
        .ok_or_else(|| {
            CellarError::Internal(
                "Missing required owner (header X-Alhalte-Owner or query parameter owner)"
                    .to_string(),
            )
        })?;

    // Extract session_id from header or query param
    let session_id = headers
        .get("X-Alhalte-Session-Id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query.session_id)
        .ok_or_else(|| {
            CellarError::Internal("Missing required session_id (header X-Alhalte-Session-Id or query parameter session_id)".to_string())
        })?;

    // Extract wrapped_key from header or query param
    let wrapped_key_hex = headers
        .get("X-Alhalte-Wrapped-Key")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string)
        .or(query.wrapped_key);

    let wrapped_key = match wrapped_key_hex {
        Some(s) if !s.is_empty() => Some(hex::decode(s)?),
        _ => None,
    };

    let result = service
        .store_payload(owner, session_id, tin_id, &body, wrapped_key)
        .await?;

    Ok((StatusCode::CREATED, Json(result)).into_response())
}

/// Download handler returning raw tin binary and optional wrapped key header.
async fn handle_download(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<DownloadQuery>,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    let owner = headers
        .get("X-Alhalte-Owner")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query.owner);

    let (data, wrapped_key) = service.download(&tin_id, owner).await?;

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        CONTENT_TYPE,
        header::HeaderValue::from_static("application/octet-stream"),
    );

    if let Some(key) = wrapped_key {
        let hex_key = hex::encode(key);
        if let Ok(val) = header::HeaderValue::from_str(&hex_key) {
            response_headers.insert("X-Alhalte-Wrapped-Key", val);
        }
    }

    Ok((StatusCode::OK, response_headers, data).into_response())
}

/// Metadata query handler.
async fn handle_tin_metadata(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<DownloadQuery>,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    let owner = headers
        .get("X-Alhalte-Owner")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query.owner);

    let (data, wrapped_key) = service.download(&tin_id, owner).await?;

    let meta = serde_json::json!({
        "tin_id": format_tin_id(&tin_id),
        "size": data.len(),
        "wrapped_key": wrapped_key.as_ref().map(hex::encode),
    });

    Ok((StatusCode::OK, Json(meta)).into_response())
}

/// Unregister handler via JSON POST.
async fn handle_unregister(
    State(service): State<Arc<CellarService>>,
    Json(payload): Json<UnregisterRequest>,
) -> std::result::Result<Response, CellarError> {
    let deleted = service
        .unregister(payload.owner, payload.session_id, &payload.tin_id)
        .await?;

    if deleted {
        Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "unregistered": true })),
        )
            .into_response())
    } else {
        Err(CellarError::NotFound(format_tin_id(&payload.tin_id)))
    }
}

/// Unregister handler via DELETE.
async fn handle_unregister_delete(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<UploadQuery>,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    let owner = headers
        .get("X-Alhalte-Owner")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query.owner)
        .ok_or_else(|| {
            CellarError::Internal(
                "Missing required owner (header X-Alhalte-Owner or query parameter owner)"
                    .to_string(),
            )
        })?;

    let session_id = headers
        .get("X-Alhalte-Session-Id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(query.session_id)
        .ok_or_else(|| {
            CellarError::Internal("Missing required session_id (header X-Alhalte-Session-Id or query parameter session_id)".to_string())
        })?;

    let deleted = service.unregister(owner, session_id, &tin_id).await?;

    if deleted {
        Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "unregistered": true })),
        )
            .into_response())
    } else {
        Err(CellarError::NotFound(format_tin_id(&tin_id)))
    }
}

/// Garbage collection trigger handler.
async fn handle_gc(
    State(service): State<Arc<CellarService>>,
) -> std::result::Result<Response, CellarError> {
    let stats = service.run_gc().await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}
