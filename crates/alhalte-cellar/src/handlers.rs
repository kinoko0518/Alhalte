//! Axum HTTP route handlers and router configuration for `alhalte-cellar`.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Path as AxPath, Query, State};
use axum::http::header::{CONTENT_TYPE, HeaderMap};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bytes::Bytes;
use serde::Deserialize;

use crate::error::CellarError;
use crate::model::{
    AliasRequest, AliasResponse, StoreRequest, StoreResponse, UnregisterRequest, WriteCredentials,
    format_tin_id, parse_tin_id,
};
use crate::service::CellarService;

/// Maximum accepted request body for tin uploads (64 MiB).
///
/// Axum's default limit is 2 MiB, which rejects ordinary room assets and
/// extension wasm. The proxy in `alhalte-maitre-d` uses the same limit so a
/// payload that passes the gateway is not rejected by the storage service.
/// Bodies are still buffered fully in memory by the `Bytes` extractor.
pub const MAX_STORE_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Header name carrying the internal service token.
///
/// `alhalte-banquette` uses it for room state persistence so that internal
/// writes do not need a synthetic user session. The value is never logged.
pub const SERVICE_TOKEN_HEADER: &str = "X-Alhalte-Service-Token";

/// Header name carrying the reference label (owner) of a write.
///
/// `alhalte-maitre-d` always sets it from the authenticated session; the request
/// body never carries an ownership claim (Issue #191).
pub const OWNER_HEADER: &str = "X-Alhalte-Owner";

/// Header name carrying the session ID that authorizes a user write.
pub const SESSION_ID_HEADER: &str = "X-Alhalte-Session-Id";

/// Reads the internal service token from request headers, if present.
///
/// An empty header value is treated as absent so a misconfigured client cannot
/// accidentally authorize with an empty configured token.
fn service_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(SERVICE_TOKEN_HEADER)?.to_str().ok()?;
    if value.is_empty() { None } else { Some(value) }
}

/// Reads a `u64` from a request header, ignoring absent or malformed values.
fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Reads the write credentials from the request headers.
///
/// Only [`OWNER_HEADER`] / [`SESSION_ID_HEADER`] are accepted: query parameters
/// and request bodies never authenticate a write (Issue #191).
fn credentials_from_headers(headers: &HeaderMap) -> WriteCredentials {
    WriteCredentials {
        owner: header_u64(headers, OWNER_HEADER),
        session_id: header_u64(headers, SESSION_ID_HEADER),
    }
}

/// Maps an Axum JSON rejection to a `400 Bad Request`.
///
/// Axum answers `422 Unprocessable Entity` for a body that deserializes badly,
/// but a legacy `owner` / `session_id` field (or any other unknown field) is a
/// malformed request for cellar, so the contract is a plain 400.
fn json_rejection(rejection: JsonRejection) -> CellarError {
    CellarError::InvalidBody(rejection.to_string())
}

/// Query parameters for download requests.
#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// Optional owner filter.
    pub owner: Option<u64>,
}

/// Query parameters for upload requests.
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// Optional wrapped key in hex.
    pub wrapped_key: Option<String>,
}

/// Helper to convert `CellarError` into an appropriate HTTP response.
impl IntoResponse for CellarError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::Unauthorized { .. } => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::NotFound(_) | Self::AliasNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Self::AlreadyOwned { .. } => (StatusCode::CONFLICT, self.to_string()),
            Self::Hex(_)
            | Self::Json(_)
            | Self::InvalidBody(_)
            | Self::SizeMismatch { .. }
            | Self::InvalidAlias(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Io(_)
            | Self::Database(_)
            | Self::Tin(_)
            | Self::Verifier(_)
            | Self::Configuration(_)
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
///
/// The store upload routes accept bodies up to
/// [`MAX_STORE_BODY_BYTES`]; Axum's 2 MiB default is too small for room
/// assets and extension wasm.
pub fn create_router(service: Arc<CellarService>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/store", post(handle_store_check))
        .route(
            "/store/{tin_id}",
            put(handle_store_upload)
                .post(handle_store_upload)
                .delete(handle_unregister_delete)
                .layer(DefaultBodyLimit::max(MAX_STORE_BODY_BYTES)),
        )
        .route(
            "/tin/{tin_id}",
            put(handle_store_upload)
                .post(handle_store_upload)
                .get(handle_download)
                .delete(handle_unregister_delete)
                .layer(DefaultBodyLimit::max(MAX_STORE_BODY_BYTES)),
        )
        .route("/tin/{tin_id}/meta", get(handle_tin_metadata))
        .route("/download/{tin_id}", get(handle_download))
        .route("/download/{tin_id}/meta", get(handle_tin_metadata))
        .route("/unregister", post(handle_unregister))
        .route(
            "/alias/{*name}",
            get(handle_get_alias)
                .put(handle_put_alias)
                .delete(handle_delete_alias),
        )
        .route("/gc", post(handle_gc))
        .with_state(service)
}

/// Healthcheck endpoint.
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

/// Store metadata check handler.
///
/// Always returns `200 OK` with a [`StoreResponse`] body whose `result` is
/// `upload_required`, `deduplicated`, or `already_owned`; errors keep their
/// usual status (401 / 400 / 5xx). Authorization is either the internal service
/// token (see [`SERVICE_TOKEN_HEADER`]) or the owner's user session, both taken
/// from request headers. A body carrying `owner` / `session_id` is rejected with
/// 400.
async fn handle_store_check(
    State(service): State<Arc<CellarService>>,
    headers: HeaderMap,
    payload: std::result::Result<Json<StoreRequest>, JsonRejection>,
) -> std::result::Result<Response, CellarError> {
    let Json(payload) = payload.map_err(json_rejection)?;

    let result = service
        .store_check_with_token(
            &payload,
            credentials_from_headers(&headers),
            service_token_from_headers(&headers),
        )
        .await?;

    Ok((StatusCode::OK, Json(StoreResponse { result })).into_response())
}

/// Tin payload upload handler.
///
/// Identity comes from the `X-Alhalte-Owner` / `X-Alhalte-Session-Id` headers or
/// the service token; query parameters are only used for `wrapped_key`.
async fn handle_store_upload(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<UploadQuery>,
    body: Bytes,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    // A service-token upload needs neither header: the token is the credential.
    let service_token = service_token_from_headers(&headers);
    let credentials = credentials_from_headers(&headers);

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
        .store_payload_with_token(tin_id, &body, wrapped_key, credentials, service_token)
        .await?;

    Ok((StatusCode::CREATED, Json(result)).into_response())
}

/// Download handler returning raw tin binary and optional wrapped key header.
///
/// Download never authorizes: `X-Alhalte-Owner` (or `?owner=`) only narrows the
/// reference row that is looked up.
async fn handle_download(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<DownloadQuery>,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    let owner = header_u64(&headers, OWNER_HEADER).or(query.owner);

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

    let owner = header_u64(&headers, OWNER_HEADER).or(query.owner);

    let (data, wrapped_key) = service.download(&tin_id, owner).await?;

    let meta = serde_json::json!({
        "tin_id": format_tin_id(&tin_id),
        "size": data.len(),
        "wrapped_key": wrapped_key.as_ref().map(hex::encode),
    });

    Ok((StatusCode::OK, Json(meta)).into_response())
}

/// Unregister handler via JSON POST.
///
/// Authorization is either the internal service token or the owner's session,
/// both taken from request headers. A body carrying `owner` / `session_id` is
/// rejected with 400.
async fn handle_unregister(
    State(service): State<Arc<CellarService>>,
    headers: HeaderMap,
    payload: std::result::Result<Json<UnregisterRequest>, JsonRejection>,
) -> std::result::Result<Response, CellarError> {
    let Json(payload) = payload.map_err(json_rejection)?;

    let deleted = service
        .unregister_with_token(
            &payload.tin_id,
            credentials_from_headers(&headers),
            service_token_from_headers(&headers),
        )
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
///
/// Identity comes from the request headers only; a `?owner=` / `?session_id=`
/// query parameter is not accepted.
async fn handle_unregister_delete(
    State(service): State<Arc<CellarService>>,
    AxPath(tin_id_hex): AxPath<String>,
    headers: HeaderMap,
) -> std::result::Result<Response, CellarError> {
    let tin_id = parse_tin_id(&tin_id_hex)?;

    let deleted = service
        .unregister_with_token(
            &tin_id,
            credentials_from_headers(&headers),
            service_token_from_headers(&headers),
        )
        .await?;

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

/// Generic alias read handler (`GET /alias/{name}`).
///
/// The name is a wildcard path segment, so hierarchical names such as
/// `room/<room_id>/head` work without escaping the slashes. Reads are
/// unauthenticated, like downloads. An unset alias is `404`.
async fn handle_get_alias(
    State(service): State<Arc<CellarService>>,
    AxPath(name): AxPath<String>,
) -> std::result::Result<Response, CellarError> {
    let tin_id = service
        .get_alias(&name)
        .await?
        .ok_or(CellarError::AliasNotFound(name))?;

    Ok((StatusCode::OK, Json(AliasResponse { tin_id })).into_response())
}

/// Generic alias write handler (`PUT /alias/{name}`).
///
/// Requires the internal service token; a user session is rejected because an
/// alias is shared infrastructure state, not a user-owned reference.
async fn handle_put_alias(
    State(service): State<Arc<CellarService>>,
    AxPath(name): AxPath<String>,
    headers: HeaderMap,
    Json(payload): Json<AliasRequest>,
) -> std::result::Result<Response, CellarError> {
    service
        .set_alias_with_token(&name, &payload.tin_id, service_token_from_headers(&headers))
        .await?;

    Ok((
        StatusCode::OK,
        Json(AliasResponse {
            tin_id: payload.tin_id,
        }),
    )
        .into_response())
}

/// Generic alias delete handler (`DELETE /alias/{name}`).
///
/// Requires the internal service token. The answer reports whether a row was
/// removed so a repeated delete is not an error. Deleting an alias releases the
/// tin for the next GC when no other reference remains.
async fn handle_delete_alias(
    State(service): State<Arc<CellarService>>,
    AxPath(name): AxPath<String>,
    headers: HeaderMap,
) -> std::result::Result<Response, CellarError> {
    let deleted = service
        .delete_alias_with_token(&name, service_token_from_headers(&headers))
        .await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "deleted": deleted })),
    )
        .into_response())
}
