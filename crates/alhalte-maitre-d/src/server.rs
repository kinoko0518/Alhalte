//! HTTP and WebSocket gateway server routes and handlers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use alhalte_banquette::{ClientFrame, ServerFrame};

use crate::crypto;
use crate::db::{Account, Database, Room, RoomRole};
use crate::error::MaitreDError;
use crate::gateway::RoomBridgeManager;
use crate::oauth::OAuthProvider;
use crate::room_id::RoomId;

/// Application shared state.
#[derive(Clone)]
pub struct AppState {
    /// Database manager.
    pub db: Database,
    /// Instance salt for Google `sub` encryption.
    pub salt: String,
    /// OAuth provider (production or mock).
    pub oauth: Arc<dyn OAuthProvider>,
    /// WebSocket room demuxer manager.
    pub room_manager: RoomBridgeManager,
    /// Optional authenticated reverse proxy to `alhalte-cellar` (`/api/cellar/*`).
    ///
    /// When `None`, the proxied routes answer `503 Service Unavailable`; no other
    /// route is affected.
    pub cellar: Option<CellarProxy>,
    /// Client ID monotonic counter.
    client_counter: Arc<AtomicU64>,
}

impl AppState {
    /// Creates a new [`AppState`] without a cellar proxy.
    ///
    /// Use [`AppState::with_cellar_proxy`] (or [`AppState::with_cellar_url`]) to
    /// enable the `/api/cellar/*` routes.
    pub fn new(
        db: Database,
        salt: impl Into<String>,
        oauth: Arc<dyn OAuthProvider>,
        room_manager: RoomBridgeManager,
    ) -> Self {
        Self {
            db,
            salt: salt.into(),
            oauth,
            room_manager,
            cellar: None,
            client_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Attaches an authenticated cellar reverse proxy configuration.
    #[must_use]
    pub fn with_cellar_proxy(mut self, proxy: CellarProxy) -> Self {
        self.cellar = Some(proxy);
        self
    }

    /// Attaches a cellar reverse proxy pointing at `cellar_url` (`--cellar-url`).
    ///
    /// # Errors
    ///
    /// Returns [`CellarProxyError::InvalidBaseUrl`] when `cellar_url` is not an
    /// absolute `http` / `https` URL, or [`CellarProxyError::Client`] when the
    /// underlying HTTP client cannot be built.
    pub fn with_cellar_url(self, cellar_url: impl Into<String>) -> Result<Self, CellarProxyError> {
        Ok(self.with_cellar_proxy(CellarProxy::new(cellar_url)?))
    }

    /// Allocates a new unique client ID for a WebSocket connection.
    pub fn next_client_id(&self) -> u64 {
        self.client_counter.fetch_add(1, Ordering::Relaxed)
    }
}

/// Extracts session ID from the HTTP `Cookie` header (`session_id=...`).
pub fn extract_session_id_from_cookie(headers: &HeaderMap) -> Option<u64> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    parse_session_id_from_cookie_str(cookie_header)
}

/// Parses a 64-bit session ID from a Cookie header string.
pub fn parse_session_id_from_cookie_str(cookie_str: &str) -> Option<u64> {
    for pair in cookie_str.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        if let (Some(name), Some(val)) = (parts.next(), parts.next())
            && name.trim() == "session_id"
        {
            let clean_val = val.trim().trim_matches('"');
            if let Ok(id) = clean_val.parse::<u64>() {
                return Some(id);
            }
        }
    }
    None
}

/// Resolves and verifies the caller account for an authenticated HTTP request.
///
/// The session is taken from the query, the `X-Alhalte-Session` header, or the
/// `session_id` cookie. When the account is not claimed explicitly it is derived
/// from the session. Both values must agree with an active session row.
fn resolve_authenticated_account(
    state: &AppState,
    headers: &HeaderMap,
    claimed_account_id: Option<u64>,
    claimed_session_id: Option<u64>,
) -> std::result::Result<u64, (StatusCode, Json<serde_json::Value>)> {
    let session_id = claimed_session_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Session")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .or_else(|| extract_session_id_from_cookie(headers));

    let Some(session_id) = session_id else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Missing session ID" })),
        ));
    };

    let account_id = match claimed_account_id.or_else(|| {
        headers
            .get("X-Alhalte-Account")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
    }) {
        Some(account_id) => account_id,
        None => match state.db.get_session(session_id) {
            Ok(Some(session)) => session.account_id,
            Ok(None) => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Invalid session or account" })),
                ));
            }
            Err(error) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": error.to_string() })),
                ));
            }
        },
    };

    match state.db.is_session_valid(account_id, session_id) {
        Ok(true) => Ok(account_id),
        Ok(false) => Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Invalid session or account" })),
        )),
        Err(error) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        )),
    }
}

/// Query parameters for session verification.
#[derive(Debug, Deserialize, Default)]
pub struct VerifySessionQuery {
    /// Owning account ID (also accepts `account_id`).
    pub owner: Option<u64>,
    /// Owning account ID.
    pub account_id: Option<u64>,
    /// Session ID to verify.
    pub session_id: Option<u64>,
}

/// Response payload for session verification.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifySessionResponse {
    /// Whether session is valid for the account.
    pub valid: bool,
    /// Account ID.
    pub account_id: u64,
    /// Session ID.
    pub session_id: u64,
}

/// Request payload for heartbeat.
#[derive(Debug, Deserialize, Default)]
pub struct HeartbeatRequest {
    /// Session ID.
    pub session_id: Option<u64>,
}

/// Query parameters for searching accounts.
#[derive(Debug, Deserialize, Default)]
pub struct SearchAccountsQuery {
    /// Search keyword (handle name or numeric account ID).
    pub q: Option<String>,
}

/// Search result item for accounts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSearchItem {
    /// 64-bit account ID.
    pub account_id: u64,
    /// Username / handle name.
    pub username: String,
    /// Avatar icon URL if present.
    pub icon_url: Option<String>,
}

/// Request payload for setting room permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRoomPermissionRequest {
    /// Account ID to assign permission.
    pub account_id: u64,
    /// Assigned role in the room.
    pub role: crate::db::RoomRole,
}

/// Query parameters for OAuth redirect URL generation.
#[derive(Debug, Deserialize)]
pub struct OAuthUrlQuery {
    /// OAuth redirect URI.
    pub redirect_uri: String,
    /// Random CSRF state string.
    pub state: String,
}

/// Request payload for OAuth callback exchange.
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackRequest {
    /// Authorization code returned from Google.
    pub code: String,
    /// Redirect URI matching the authorization request.
    pub redirect_uri: String,
    /// Optional default handle name.
    pub handle_name: Option<String>,
}

/// Request payload for OAuth ID token verification.
#[derive(Debug, Deserialize)]
pub struct OAuthTokenRequest {
    /// Google ID token.
    pub id_token: String,
    /// Optional default handle name.
    pub handle_name: Option<String>,
}

/// Authentication response containing account and session ID.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Account entity.
    pub account: Account,
    /// Issued 64-bit session ID.
    pub session_id: u64,
}

/// Request payload for profile update.
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    /// Session ID for authorization.
    pub session_id: Option<u64>,
    /// New handle name.
    pub handle_name: Option<String>,
    /// New avatar icon Tin ID encoded as a 64-character lowercase hex string (or null).
    pub icon_tin_id: Option<Option<String>>,
}

/// Request payload for debug account creation.
#[cfg(debug_assertions)]
#[derive(Debug, Deserialize)]
pub struct CreateDebugAccountRequest {
    /// Handle name.
    pub handle_name: String,
    /// Optional avatar icon Tin ID hex string.
    pub icon_tin_id: Option<String>,
}

/// Query parameters for WebSocket connection.
#[derive(Debug, Deserialize, Default)]
pub struct WsQuery {
    /// Room identifier (16 hex or legacy decimal).
    pub room: Option<RoomId>,
    /// Account identifier.
    pub account_id: Option<u64>,
    /// Session identifier.
    pub session_id: Option<u64>,
}

/// Query parameters for room ZIP archive export.
#[derive(Debug, Deserialize, Default)]
pub struct ExportZipQuery {
    /// Export type: "full" (default) or "logs".
    pub r#type: Option<String>,
    /// Optional authenticated account ID.
    pub account_id: Option<u64>,
    /// Optional authenticated session ID.
    pub session_id: Option<u64>,
}

/// Maximum accepted room name length, in Unicode scalar values.
pub const MAX_ROOM_NAME_LEN: usize = 100;

/// Query parameters carrying the caller session for room registry routes.
#[derive(Debug, Deserialize, Default)]
pub struct RoomSessionQuery {
    /// Optional authenticated account ID.
    pub account_id: Option<u64>,
    /// Optional authenticated session ID.
    pub session_id: Option<u64>,
}

/// Request payload for `POST /api/rooms`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateRoomRequest {
    /// Optional canonical 16-hex room ID. Generated server-side when omitted.
    pub room_id: Option<String>,
    /// Human-readable room name.
    pub name: String,
}

/// Room metadata returned by the registry routes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomInfoResponse {
    /// Canonical 16-hex room ID.
    pub room_id: String,
    /// Human-readable room name.
    pub name: String,
    /// Caller role in the room (`gm`, `pl`, or `viewer`).
    pub role: RoomRole,
    /// Whether the room has been marked as done (fully public).
    pub done: bool,
}

/// Maximum number of random room IDs tried when the caller omits `room_id`.
const MAX_ROOM_ID_GENERATION_ATTEMPTS: usize = 16;

/// Maximum accepted request body for cellar transfer routes (64 MiB).
///
/// Tin payloads (room assets, extension wasm, log chunks) exceed Axum's 2 MiB
/// default. The proxy and `alhalte-cellar` share this limit; the body is still
/// buffered fully in memory by the `Bytes` extractor.
pub const MAX_CELLAR_TRANSFER_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Creates the Axum router for `alhalte-maitre-d`.
pub fn create_router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/api/verify-session", get(verify_session_handler))
        .route("/api/heartbeat", post(heartbeat_handler))
        .route("/auth/google/url", get(oauth_url_handler))
        .route(
            "/auth/google/callback",
            get(oauth_callback_get_handler).post(oauth_callback_post_handler),
        )
        .route("/auth/google/token", post(oauth_token_handler))
        .route(
            "/api/account/{account_id}",
            get(get_account_handler).patch(update_account_handler),
        )
        // Account search
        .route("/api/accounts/search", get(search_accounts_handler))
        .route("/accounts/search", get(search_accounts_handler))
        // Room registry
        .route("/api/rooms", post(create_room_handler))
        .route("/api/rooms/{room_id}", get(get_room_info_handler))
        // Room permissions
        .route(
            "/api/room/{room_id}/permissions",
            get(get_room_permissions_handler).post(set_room_permission_handler),
        )
        .route(
            "/room/{room_id}/permissions",
            get(get_room_permissions_handler).post(set_room_permission_handler),
        )
        .route(
            "/api/room/{room_id}/permissions/{account_id}",
            axum::routing::delete(delete_room_permission_handler),
        )
        .route(
            "/room/{room_id}/permissions/{account_id}",
            axum::routing::delete(delete_room_permission_handler),
        )
        // WebSocket endpoints
        .route("/ws", get(ws_handler))
        .route("/ws/room/{room_id}", get(ws_room_handler))
        .route("/room/{room_id}", get(ws_room_handler))
        .route("/room/{room_id}/ws", get(ws_room_handler))
        // ZIP export endpoints
        .route(
            "/api/room/{room_id}/export/zip",
            get(export_room_zip_handler),
        )
        .route("/room/{room_id}/export/zip", get(export_room_zip_handler))
        // Authenticated cellar reverse proxy (see `CellarProxy`).
        //
        // Tin payloads can be far larger than Axum's 2 MiB default (a room
        // asset or extension wasm), so the store routes raise the per-route
        // body limit. Everything else keeps the default.
        .route(
            "/api/cellar/store",
            post(cellar_store_check_handler)
                .layer(DefaultBodyLimit::max(MAX_CELLAR_TRANSFER_BODY_BYTES)),
        )
        .route(
            "/api/cellar/store/{tin_id}",
            post(cellar_store_upload_handler)
                .put(cellar_store_upload_handler)
                .layer(DefaultBodyLimit::max(MAX_CELLAR_TRANSFER_BODY_BYTES)),
        )
        .route("/api/cellar/tin/{tin_id}", get(cellar_download_handler));

    #[cfg(debug_assertions)]
    {
        router = router.route("/api/debug/account", post(create_debug_account_handler));
    }

    router
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

/// Handles `GET /api/verify-session`.
///
/// Compatible with `alhalte-cellar`'s `MaitreDVerifier`.
async fn verify_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<VerifySessionQuery>,
) -> Response {
    let session_id = q
        .session_id
        .or_else(|| extract_session_id_from_cookie(&headers));
    let session_id = match session_id {
        Some(sid) => sid,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing owner/account_id parameter" })),
            )
                .into_response();
        }
    };

    let account_id = match q.owner.or(q.account_id) {
        Some(id) => id,
        None => match state.db.get_session(session_id) {
            Ok(Some(s)) => s.account_id,
            Ok(None) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "valid": false,
                        "message": "Invalid session or account"
                    })),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        },
    };

    match state.db.is_session_valid(account_id, session_id) {
        Ok(true) => (
            StatusCode::OK,
            Json(VerifySessionResponse {
                valid: true,
                account_id,
                session_id,
            }),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "valid": false,
                "message": "Invalid session or account"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `POST /api/heartbeat`.
async fn heartbeat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<HeartbeatRequest>,
) -> Response {
    let session_id = req
        .session_id
        .or_else(|| extract_session_id_from_cookie(&headers));
    let session_id = match session_id {
        Some(sid) => sid,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing session ID" })),
            )
                .into_response();
        }
    };

    let now = chrono::Utc::now().timestamp();
    match state.db.record_heartbeat(session_id, now) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Session not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `GET /auth/google/url`.
async fn oauth_url_handler(
    State(state): State<AppState>,
    Query(q): Query<OAuthUrlQuery>,
) -> Response {
    let url = state.oauth.authorization_url(&q.redirect_uri, &q.state);
    Json(serde_json::json!({ "url": url })).into_response()
}

/// Handles `GET /auth/google/callback`.
async fn oauth_callback_get_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let code = match params.get("code") {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing code" })),
            )
                .into_response();
        }
    };
    let redirect_uri = params.get("redirect_uri").cloned().unwrap_or_default();
    let handle_name = params.get("handle_name").cloned();

    handle_oauth_exchange(&state, code, &redirect_uri, handle_name.as_deref()).await
}

/// Handles `POST /auth/google/callback`.
async fn oauth_callback_post_handler(
    State(state): State<AppState>,
    Json(req): Json<OAuthCallbackRequest>,
) -> Response {
    handle_oauth_exchange(
        &state,
        &req.code,
        &req.redirect_uri,
        req.handle_name.as_deref(),
    )
    .await
}

async fn handle_oauth_exchange(
    state: &AppState,
    code: &str,
    redirect_uri: &str,
    handle_name: Option<&str>,
) -> Response {
    let user_info = match state.oauth.exchange_code(code, redirect_uri).await {
        Ok(info) => info,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let encrypted_sub = match crypto::encrypt_sub(&state.salt, &user_info.sub) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let now = chrono::Utc::now().timestamp();
    let default_name = handle_name.unwrap_or("Anonymous");

    let account = match state
        .db
        .get_or_create_account(&encrypted_sub, default_name, None, now)
    {
        Ok(acc) => acc,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let session = match state.db.create_session(account.account_id, now) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    Json(AuthResponse {
        account,
        session_id: session.session_id,
    })
    .into_response()
}

/// Handles `POST /auth/google/token`.
async fn oauth_token_handler(
    State(state): State<AppState>,
    Json(req): Json<OAuthTokenRequest>,
) -> Response {
    let user_info = match state.oauth.verify_id_token(&req.id_token).await {
        Ok(info) => info,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let encrypted_sub = match crypto::encrypt_sub(&state.salt, &user_info.sub) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let now = chrono::Utc::now().timestamp();
    let default_name = req.handle_name.as_deref().unwrap_or("Anonymous");

    let account = match state
        .db
        .get_or_create_account(&encrypted_sub, default_name, None, now)
    {
        Ok(acc) => acc,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let session = match state.db.create_session(account.account_id, now) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    Json(AuthResponse {
        account,
        session_id: session.session_id,
    })
    .into_response()
}

/// Handles `GET /api/account/{account_id}`.
async fn get_account_handler(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<u64>,
) -> Response {
    match state.db.get_account(account_id) {
        Ok(Some(account)) => Json(account).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Account not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `PATCH /api/account/{account_id}`.
async fn update_account_handler(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<u64>,
    headers: HeaderMap,
    Json(req): Json<UpdateProfileRequest>,
) -> Response {
    let session_id = req
        .session_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Session")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        })
        .or_else(|| extract_session_id_from_cookie(&headers));

    let session_id = match session_id {
        Some(sid) => sid,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Missing session ID" })),
            )
                .into_response();
        }
    };

    match state.db.is_session_valid(account_id, session_id) {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid session" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    let parsed_icon = match req.icon_tin_id {
        Some(Some(hex_str)) => match hex::decode(&hex_str) {
            Ok(bytes) if bytes.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Some(Some(arr))
            }
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Invalid icon_tin_id hex string" })),
                )
                    .into_response();
            }
        },
        Some(None) => Some(None),
        None => None,
    };

    match state
        .db
        .update_account_profile(account_id, req.handle_name.as_deref(), parsed_icon)
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `POST /api/debug/account` (debug builds only).
#[cfg(debug_assertions)]
async fn create_debug_account_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateDebugAccountRequest>,
) -> Response {
    let now = chrono::Utc::now().timestamp();
    let icon_bytes = req.icon_tin_id.and_then(|h| {
        hex::decode(&h).ok().and_then(|b| {
            if b.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&b);
                Some(arr)
            } else {
                None
            }
        })
    });

    let account = match state
        .db
        .create_debug_account(&req.handle_name, icon_bytes, now)
    {
        Ok(acc) => acc,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let session = match state.db.create_session(account.account_id, now) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    Json(AuthResponse {
        account,
        session_id: session.session_id,
    })
    .into_response()
}

/// Handles `GET /api/accounts/search?q={query}`.
async fn search_accounts_handler(
    State(state): State<AppState>,
    Query(query): Query<SearchAccountsQuery>,
) -> Response {
    let q = query.q.as_deref().unwrap_or("").trim();
    if q.is_empty() {
        return (StatusCode::OK, Json(Vec::<AccountSearchItem>::new())).into_response();
    }

    match state.db.search_accounts(q) {
        Ok(accounts) => {
            let items: Vec<AccountSearchItem> = accounts
                .into_iter()
                .map(|acc| AccountSearchItem {
                    account_id: acc.account_id,
                    username: acc.handle_name,
                    icon_url: acc
                        .icon_tin_id
                        .map(|id| format!("/api/cellar/tin/{}", hex::encode(id))),
                })
                .collect();
            (StatusCode::OK, Json(items)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Validates and normalizes a room name supplied to `POST /api/rooms`.
fn validate_room_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty()
        || name.chars().count() > MAX_ROOM_NAME_LEN
        || name.chars().any(char::is_control)
    {
        None
    } else {
        Some(name.to_string())
    }
}

/// Generates a non-zero random room ID for `POST /api/rooms` when omitted.
fn generate_room_id() -> RoomId {
    loop {
        let value = rand::random::<u64>();
        if value != 0 {
            return RoomId::new(value);
        }
    }
}

/// Builds the success response for a newly registered room.
fn room_created_response(room: Room) -> Response {
    (
        StatusCode::OK,
        Json(RoomInfoResponse {
            room_id: room.room_id,
            name: room.name,
            role: RoomRole::Gm,
            done: false,
        }),
    )
        .into_response()
}

/// Registers a caller-supplied room ID, mapping a duplicate to `409 Conflict`.
fn create_requested_room(
    state: &AppState,
    room_id: RoomId,
    name: &str,
    creator_account: u64,
    now: i64,
) -> Response {
    match state
        .db
        .create_room(&room_id.to_string(), name, creator_account, now)
    {
        Ok(room) => room_created_response(room),
        Err(MaitreDError::RoomAlreadyExists(_)) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Room already exists" })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `POST /api/rooms`.
///
/// Requires an authenticated session. Registers the room and the caller as its
/// initial `gm`; no banquette process is spawned until the room is joined.
async fn create_room_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RoomSessionQuery>,
    Json(req): Json<CreateRoomRequest>,
) -> Response {
    let caller =
        match resolve_authenticated_account(&state, &headers, query.account_id, query.session_id) {
            Ok(account_id) => account_id,
            Err(response) => return response.into_response(),
        };

    let Some(name) = validate_room_name(&req.name) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "Room name must be non-empty, at most {MAX_ROOM_NAME_LEN} characters, and contain no control characters"
                )
            })),
        )
            .into_response();
    };

    let now = chrono::Utc::now().timestamp();

    if let Some(raw) = req.room_id.as_deref() {
        let Ok(room_id) = raw.trim().parse::<RoomId>() else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "room_id must be a 16-hex room ID"
                })),
            )
                .into_response();
        };
        return create_requested_room(&state, room_id, &name, caller, now);
    }

    for _ in 0..MAX_ROOM_ID_GENERATION_ATTEMPTS {
        let room_id = generate_room_id();
        match state
            .db
            .create_room(&room_id.to_string(), &name, caller, now)
        {
            Ok(room) => return room_created_response(room),
            Err(MaitreDError::RoomAlreadyExists(_)) => continue,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": error.to_string() })),
                )
                    .into_response();
            }
        }
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "Failed to allocate a unique room ID" })),
    )
        .into_response()
}

/// Handles `GET /api/rooms/{room_id}`.
///
/// Returns the room metadata for members. Non-members may read a room that is
/// marked done (`role = "viewer"`); otherwise the response is `403 Forbidden`.
async fn get_room_info_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<RoomId>,
    headers: HeaderMap,
    Query(query): Query<RoomSessionQuery>,
) -> Response {
    let caller =
        match resolve_authenticated_account(&state, &headers, query.account_id, query.session_id) {
            Ok(account_id) => account_id,
            Err(response) => return response.into_response(),
        };

    let room_id_str = room_id.to_string();
    let room = match state.db.get_room(&room_id_str) {
        Ok(Some(room)) => room,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Room not found" })),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };

    let done = state.room_manager.is_room_done(room_id.as_u64()).await;
    let role = match state.db.get_room_role(&room_id_str, caller) {
        Ok(Some(role)) => role,
        Ok(None) if done => RoomRole::Viewer,
        Ok(None) => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください"
                })),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };

    Json(RoomInfoResponse {
        room_id: room.room_id,
        name: room.name,
        role,
        done,
    })
    .into_response()
}

/// Handles `GET /api/room/{room_id}/permissions`.
async fn get_room_permissions_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<RoomId>,
) -> Response {
    match state.db.get_room_permissions(&room_id.to_string()) {
        Ok(perms) => (StatusCode::OK, Json(perms)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `POST /api/room/{room_id}/permissions`.
async fn set_room_permission_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<RoomId>,
    Json(req): Json<SetRoomPermissionRequest>,
) -> Response {
    match state.db.get_account(req.account_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Account not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    let now = chrono::Utc::now().timestamp();
    let room_id_str = room_id.to_string();
    match state
        .db
        .set_room_permission(&room_id_str, req.account_id, req.role, now)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "room_id": room_id_str,
                "account_id": req.account_id,
                "role": req.role
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Handles `DELETE /api/room/{room_id}/permissions/{account_id}`.
async fn delete_room_permission_handler(
    State(state): State<AppState>,
    AxumPath((room_id, account_id)): AxumPath<(RoomId, u64)>,
) -> Response {
    match state
        .db
        .delete_room_permission(&room_id.to_string(), account_id)
    {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Checks whether a [`ClientFrame`] mutates room state, scene structure, or logs.
pub fn is_mutate_frame(frame: &ClientFrame) -> bool {
    matches!(
        frame,
        ClientFrame::AppendLog { .. }
            | ClientFrame::EditLog { .. }
            | ClientFrame::DeleteLog { .. }
            | ClientFrame::RewindLog { .. }
            | ClientFrame::UpdateState { .. }
            | ClientFrame::DeleteState { .. }
            | ClientFrame::CreateScene { .. }
            | ClientFrame::DeleteScene { .. }
            | ClientFrame::SetDone { .. }
            | ClientFrame::CreateCharacter { .. }
            | ClientFrame::UpdateCharacter { .. }
            | ClientFrame::DeleteCharacter { .. }
            | ClientFrame::MoveCharacter { .. }
            | ClientFrame::ChangeBgm { .. }
    )
}

/// Handles `/ws` with query or header parameters.
async fn ws_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let room_id = query.room.or_else(|| {
        headers
            .get("X-Alhalte-Room")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<RoomId>().ok())
    });

    let room_id = match room_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing room ID (via query 'room' or header 'X-Alhalte-Room')" })),
            )
                .into_response();
        }
    };

    handle_ws_upgrade(state, headers, query, room_id, ws).await
}

/// Handles `/ws/room/{room_id}`, `/room/{room_id}`, `/room/{room_id}/ws`.
async fn ws_room_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<RoomId>,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    handle_ws_upgrade(state, headers, query, room_id, ws).await
}

async fn handle_ws_upgrade(
    state: AppState,
    headers: HeaderMap,
    query: WsQuery,
    room_id: RoomId,
    ws: WebSocketUpgrade,
) -> Response {
    let session_id = query
        .session_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Session")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        })
        .or_else(|| extract_session_id_from_cookie(&headers));

    let account_id = query
        .account_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Account")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        })
        .or_else(|| {
            session_id.and_then(|sid| {
                state
                    .db
                    .get_session(sid)
                    .ok()
                    .flatten()
                    .map(|s| s.account_id)
            })
        });

    // If credentials provided, verify validity
    if let (Some(aid), Some(sid)) = (account_id, session_id) {
        match state.db.is_session_valid(aid, sid) {
            Ok(true) => {}
            Ok(false) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Invalid session credentials" })),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        }
    } else if session_id.is_some() && account_id.is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Invalid session credentials" })),
        )
            .into_response();
    }

    let auth_account = account_id.unwrap_or(0);
    let auth_session = session_id.unwrap_or(0);

    // A room must be registered in the room registry before it can be joined.
    // Permission rows alone do not create a room, and an unknown room never
    // causes a banquette process to be spawned.
    let room_id_str = room_id.to_string();
    let room_id_u64 = room_id.as_u64();
    let room = match state.db.get_room(&room_id_str) {
        Ok(Some(room)) => room,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Room not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let is_done = state.room_manager.is_room_done(room_id_u64).await;
    let user_role = if auth_account != 0 {
        match state.db.get_room_role(&room_id_str, auth_account) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    if user_role.is_none() && !is_done {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください"
            })),
        )
            .into_response();
    }

    let is_viewer = match user_role {
        Some(RoomRole::Viewer) => true,
        Some(_) => false,
        None => true,
    };

    let client_id = state.next_client_id();
    // The bridge administrator is the room creator / database GM, never the
    // account that happens to connect first.
    let admin_account = match state.db.get_room_gm(&room_id_str) {
        Ok(Some(gm_id)) => gm_id,
        Ok(None) => room.creator_account,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let bridge = match state
        .room_manager
        .get_or_create_room(room_id_u64, admin_account)
        .await
    {
        Ok(bridge) => bridge,
        Err(e) => {
            tracing::error!("Failed to start banquette for room {room_id_str}: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to restore the room process: {e}")
                })),
            )
                .into_response();
        }
    };

    let state_clone = state.clone();
    ws.on_upgrade(move |socket| {
        handle_websocket(
            socket,
            WsClientContext {
                bridge,
                state: state_clone,
                room_id,
                client_id,
                account_id: auth_account,
                session_id: auth_session,
                is_viewer_initial: is_viewer,
            },
        )
    })
}

struct WsClientContext {
    bridge: Arc<crate::gateway::RoomBridge>,
    state: AppState,
    room_id: RoomId,
    client_id: u64,
    account_id: u64,
    session_id: u64,
    is_viewer_initial: bool,
}

async fn handle_websocket(socket: WebSocket, ctx: WsClientContext) {
    let client_id = ctx.client_id;
    let account_id = ctx.account_id;
    let session_id = ctx.session_id;
    let room_id = ctx.room_id;
    let room_id_u64 = room_id.as_u64();
    let state = ctx.state;
    let bridge = ctx.bridge;
    let is_viewer_initial = ctx.is_viewer_initial;

    let mut server_frame_rx = bridge
        .register_client(client_id, account_id, session_id)
        .await;
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Outgoing task: ServerFrame -> WebSocket
    let outgoing_task = tokio::spawn(async move {
        while let Some(frame) = server_frame_rx.recv().await {
            let json_str = match serde_json::to_string(&frame) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to serialize ServerFrame to JSON: {e:?}");
                    continue;
                }
            };

            if let Err(e) = ws_sender.send(Message::text(json_str)).await {
                tracing::debug!("WebSocket client {client_id} write closed: {e:?}");
                break;
            }
        }
    });

    // Incoming task: WebSocket -> ClientFrame -> Bridge
    let bridge_ref = Arc::clone(&bridge);
    let room_id_str = room_id.to_string();

    while let Some(msg_res) = ws_receiver.next().await {
        let msg = match msg_res {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("WebSocket client {client_id} read error: {e:?}");
                break;
            }
        };

        match msg {
            Message::Text(text) => match serde_json::from_str::<ClientFrame>(&text) {
                Ok(frame) => {
                    if is_mutate_frame(&frame) {
                        let is_viewer = if account_id != 0 {
                            match state.db.get_room_role(&room_id_str, account_id) {
                                Ok(Some(RoomRole::Viewer)) => true,
                                Ok(Some(_)) => false,
                                _ => is_viewer_initial,
                            }
                        } else {
                            is_viewer_initial
                        };

                        if is_viewer {
                            bridge_ref
                                .send_to_client(
                                    client_id,
                                    ServerFrame::Error {
                                        code: "PermissionDenied".to_string(),
                                        message:
                                            "閲覧者権限のため、状態変更操作は許可されていません"
                                                .to_string(),
                                    },
                                )
                                .await;
                            continue;
                        }
                    }

                    if let ClientFrame::SetDone { done } = &frame {
                        bridge_ref.set_done(*done);
                        state.room_manager.set_room_done(room_id_u64, *done).await;
                    }

                    bridge_ref.send_client_frame(client_id, frame);
                }
                Err(e) => {
                    tracing::warn!("Failed to deserialize ClientFrame from JSON: {e:?}");
                }
            },
            Message::Binary(bytes) => {
                let frame = postcard::from_bytes::<ClientFrame>(&bytes)
                    .or_else(|_| serde_json::from_slice::<ClientFrame>(&bytes));
                match frame {
                    Ok(f) => {
                        if is_mutate_frame(&f) {
                            let is_viewer = if account_id != 0 {
                                match state.db.get_room_role(&room_id_str, account_id) {
                                    Ok(Some(RoomRole::Viewer)) => true,
                                    Ok(Some(_)) => false,
                                    _ => is_viewer_initial,
                                }
                            } else {
                                is_viewer_initial
                            };

                            if is_viewer {
                                bridge_ref
                                    .send_to_client(
                                        client_id,
                                        ServerFrame::Error {
                                            code: "PermissionDenied".to_string(),
                                            message:
                                                "閲覧者権限のため、状態変更操作は許可されていません"
                                                    .to_string(),
                                        },
                                    )
                                    .await;
                                continue;
                            }
                        }

                        if let ClientFrame::SetDone { done } = &f {
                            bridge_ref.set_done(*done);
                            state.room_manager.set_room_done(room_id_u64, *done).await;
                        }

                        bridge_ref.send_client_frame(client_id, f);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to deserialize binary ClientFrame: {e:?}");
                    }
                }
            }
            Message::Ping(payload) => {
                tracing::trace!("Received ping from client {client_id}: {payload:?}");
            }
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    outgoing_task.abort();
    bridge.unregister_client(client_id).await;
}

/// Handles `GET /api/room/{room_id}/export/zip` and `GET /room/{room_id}/export/zip`.
async fn export_room_zip_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<RoomId>,
    headers: HeaderMap,
    Query(q): Query<ExportZipQuery>,
) -> Response {
    let export_type = q.r#type.as_deref().unwrap_or("full");
    if export_type != "full" && export_type != "logs" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Invalid export type. Must be 'full' or 'logs'"
            })),
        )
            .into_response();
    }

    let session_id = q
        .session_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Session")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        })
        .or_else(|| extract_session_id_from_cookie(&headers));

    let account_id = q
        .account_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Account")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        })
        .or_else(|| {
            session_id.and_then(|sid| {
                state
                    .db
                    .get_session(sid)
                    .ok()
                    .flatten()
                    .map(|s| s.account_id)
            })
        });

    let caller_account = match (account_id, session_id) {
        (Some(aid), Some(sid)) => match state.db.is_session_valid(aid, sid) {
            Ok(true) => Some(aid),
            Ok(false) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Invalid session or account" })),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        },
        (None, None) => None,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Both account_id and session_id must be provided together"
                })),
            )
                .into_response();
        }
    };

    // Export never creates a room implicitly. Unknown rooms are rejected before
    // a banquette process is considered, and private rooms require membership.
    let room_id_str = room_id.to_string();
    let room = match state.db.get_room(&room_id_str) {
        Ok(Some(room)) => room,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Room not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let is_done = state.room_manager.is_room_done(room_id.as_u64()).await;
    let caller_role = match caller_account {
        Some(account_id) => match state.db.get_room_role(&room_id_str, account_id) {
            Ok(role) => role,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        },
        None => None,
    };

    if caller_role.is_none() && !is_done {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください"
            })),
        )
            .into_response();
    }

    let admin_account = match state.db.get_room_gm(&room_id_str) {
        Ok(Some(gm_id)) => gm_id,
        Ok(None) => room.creator_account,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // A bridge is only created for a registered room, and it restores the room
    // from the persisted head. If that head cannot be loaded, return an error
    // instead of exporting a freshly fabricated empty room.
    let bridge = match state
        .room_manager
        .get_or_create_room(room_id.as_u64(), admin_account)
        .await
    {
        Ok(bridge) => bridge,
        Err(e) => {
            tracing::error!("Failed to start banquette for export of room {room_id_str}: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to restore the room state for export: {e}")
                })),
            )
                .into_response();
        }
    };

    let (mut room_data, chunks) = match bridge.export_room(caller_account).await {
        Ok(data) => data,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    if export_type == "logs" {
        for scene in room_data.scenes.values_mut() {
            scene.states.clear();
        }
    }

    let zip_bytes = match alhalte_kitchen::export_room_to_zip(&room_data, &chunks, None) {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let filename = format!("room_{room_id}_{export_type}.zip");
    // `room_id` is rendered in its canonical 16-hex form.
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/zip"),
    );
    if let Ok(cd) =
        axum::http::HeaderValue::try_from(format!("attachment; filename=\"{filename}\""))
    {
        resp_headers.insert(axum::http::header::CONTENT_DISPOSITION, cd);
    }

    (StatusCode::OK, resp_headers, zip_bytes).into_response()
}

// ---------------------------------------------------------------------------
// Authenticated cellar reverse proxy
// ---------------------------------------------------------------------------

/// Default upstream timeout applied by [`CellarProxy::new`].
///
/// The browser side aborts at 8 s (`DEFAULT_OPERATION_TIMEOUT_MS` in
/// `web/alhalte-argenterie/src/utils/cellarTransfer.ts`); this value is only a
/// backstop so a hung `alhalte-cellar` cannot pin a maitre-d task forever.
pub const DEFAULT_CELLAR_PROXY_TIMEOUT: Duration = Duration::from_secs(30);

/// Upstream response headers that are copied back to the browser.
///
/// Everything else (including `server`, `date`, and any internal address) is
/// dropped so the proxy never leaks `alhalte-cellar` details.
const FORWARDED_RESPONSE_HEADERS: [&str; 3] = [
    "content-type",
    "content-disposition",
    "x-alhalte-wrapped-key",
];

/// Errors raised while building a [`CellarProxy`].
#[derive(Debug, thiserror::Error)]
pub enum CellarProxyError {
    /// The configured cellar base URL is not an absolute `http` / `https` URL.
    #[error("cellar base URL must be an absolute http(s) URL")]
    InvalidBaseUrl,

    /// The underlying HTTP client could not be initialized.
    #[error("failed to initialize the cellar HTTP client")]
    Client(#[source] reqwest::Error),
}

/// Configuration for the authenticated `/api/cellar/*` reverse proxy.
///
/// The browser never talks to `alhalte-cellar` directly: it calls maitre-d,
/// which resolves the caller from the session and injects the
/// `X-Alhalte-Owner` / `X-Alhalte-Session-Id` headers itself. Client-supplied
/// ownership is therefore never trusted.
#[derive(Clone, Debug)]
pub struct CellarProxy {
    /// Base URL of the cellar service, without a trailing slash.
    base_url: String,
    /// HTTP client used for upstream calls.
    client: reqwest::Client,
}

impl CellarProxy {
    /// Creates a proxy for the cellar listening at `base_url`.
    ///
    /// `base_url` may carry a path prefix (for example `http://127.0.0.1:8081`),
    /// which is prepended to every forwarded path. `/api/cellar` routes are
    /// mapped to `/store`, `/store/{tin_id}`, and `/tin/{tin_id}` on that base.
    ///
    /// # Errors
    ///
    /// Returns [`CellarProxyError::InvalidBaseUrl`] for non-absolute or
    /// non-HTTP(S) URLs and [`CellarProxyError::Client`] when the HTTP client
    /// cannot be initialized.
    pub fn new(base_url: impl Into<String>) -> Result<Self, CellarProxyError> {
        let trimmed = base_url.into().trim().trim_end_matches('/').to_string();
        let parsed = reqwest::Url::parse(&trimmed).map_err(|_| CellarProxyError::InvalidBaseUrl)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(CellarProxyError::InvalidBaseUrl);
        }

        let client = reqwest::Client::builder()
            .timeout(DEFAULT_CELLAR_PROXY_TIMEOUT)
            .build()
            .map_err(CellarProxyError::Client)?;

        Ok(Self {
            base_url: trimmed,
            client,
        })
    }

    /// Replaces the HTTP client (used by tests to shorten the upstream timeout).
    #[must_use]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// Returns the configured cellar base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Joins a cellar path (`/store`, `/tin/{id}`, ...) onto the base URL.
    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// Query parameters accepted by the proxied cellar routes.
///
/// They are used only to authenticate the caller (compatibility with the
/// existing `/api/room/{room_id}/export/zip` convention) and are **never**
/// forwarded upstream, so a client cannot smuggle `owner` / `session_id` past
/// the proxy.
#[derive(Debug, Default, Deserialize)]
pub struct CellarProxyQuery {
    /// Owning account ID claimed by the caller.
    pub owner: Option<u64>,
    /// Owning account ID claimed by the caller (alias of `owner`).
    pub account_id: Option<u64>,
    /// Session ID claimed by the caller.
    pub session_id: Option<u64>,
}

/// Failure modes when resolving the authenticated caller of a proxied request.
#[derive(Debug, thiserror::Error)]
pub enum CellarAuthError {
    /// No session identifier was supplied by cookie, header, or query.
    #[error("missing session identifier")]
    MissingSession,

    /// The session is unknown, expired, or belongs to another account.
    #[error("invalid session or account")]
    InvalidSession,

    /// The claimed account ID contradicts the session.
    #[error("account identifier does not match the session")]
    AccountMismatch,

    /// The account database could not be read.
    #[error("account lookup failed")]
    Database(#[source] crate::error::MaitreDError),
}

/// Authenticated identity forwarded to `alhalte-cellar`.
struct CallerCredentials {
    /// Account ID owning the session.
    account_id: u64,
    /// Session used to authenticate the request.
    session_id: u64,
}

/// Reads a `u64` from an `X-Alhalte-*` header.
fn header_as_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Resolves the authenticated caller from the session cookie / headers /
/// query and verifies it against the account database.
///
/// The account returned is always derived from the session row; a client
/// supplied `account_id` is only accepted when it agrees with that session.
fn resolve_caller(
    state: &AppState,
    headers: &HeaderMap,
    query: &CellarProxyQuery,
) -> Result<CallerCredentials, CellarAuthError> {
    let session_id = query
        .session_id
        .or_else(|| header_as_u64(headers, "X-Alhalte-Session"))
        .or_else(|| extract_session_id_from_cookie(headers))
        .ok_or(CellarAuthError::MissingSession)?;

    let claimed_account = query
        .owner
        .or(query.account_id)
        .or_else(|| header_as_u64(headers, "X-Alhalte-Account"));

    let session_account = state
        .db
        .get_session(session_id)
        .map_err(CellarAuthError::Database)?
        .ok_or(CellarAuthError::InvalidSession)?
        .account_id;

    if let Some(claimed) = claimed_account
        && claimed != session_account
    {
        return Err(CellarAuthError::AccountMismatch);
    }

    match state.db.is_session_valid(session_account, session_id) {
        Ok(true) => Ok(CallerCredentials {
            account_id: session_account,
            session_id,
        }),
        Ok(false) => Err(CellarAuthError::InvalidSession),
        Err(e) => Err(CellarAuthError::Database(e)),
    }
}

/// Maps a [`CellarAuthError`] to an HTTP response without leaking internals.
fn auth_error_response(err: CellarAuthError) -> Response {
    match err {
        CellarAuthError::Database(source) => {
            tracing::error!("cellar proxy account lookup failed: {source}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "account lookup failed" })),
            )
                .into_response()
        }
        CellarAuthError::AccountMismatch => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "account_id does not match the session" })),
        )
            .into_response(),
        CellarAuthError::MissingSession | CellarAuthError::InvalidSession => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid session or account" })),
        )
            .into_response(),
    }
}

/// Response returned when the proxy has no `--cellar-url` configured.
fn proxy_not_configured_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({ "error": "cellar proxy is not configured" })),
    )
        .into_response()
}

/// Maps a transport level [`reqwest::Error`] to `504` / `502`.
///
/// The upstream URL, host, and port are intentionally absent from the body.
fn upstream_error_response(err: &reqwest::Error) -> Response {
    if err.is_timeout() {
        tracing::warn!("cellar upstream timed out: {err}");
        (
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({ "error": "cellar upstream timed out" })),
        )
            .into_response()
    } else {
        tracing::warn!("cellar upstream request failed: {err}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": "cellar upstream request failed" })),
        )
            .into_response()
    }
}

/// Headers that identify the caller to `alhalte-cellar`.
fn owner_headers(caller: &CallerCredentials) -> Vec<(&'static str, String)> {
    vec![
        ("X-Alhalte-Owner", caller.account_id.to_string()),
        ("X-Alhalte-Session-Id", caller.session_id.to_string()),
    ]
}

/// Validates a 64-character hex tin ID taken from the request path.
///
/// Rejecting anything else keeps path traversal (`..`) and other upstream path
/// injections out of the proxy before a request is even built.
fn normalize_tin_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() == 64 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(trimmed.to_ascii_lowercase())
    } else {
        None
    }
}

/// Sends one request to `alhalte-cellar` and mirrors the response back.
///
/// The upstream status and body are preserved verbatim (including the
/// non-standard `210` / `211` / `212` store codes). Only the headers listed in
/// [`FORWARDED_RESPONSE_HEADERS`] are copied; transport failures become `502`
/// / `504`.
async fn forward_cellar_request(
    proxy: &CellarProxy,
    method: reqwest::Method,
    path: &str,
    headers: Vec<(&'static str, String)>,
    body: Option<Bytes>,
) -> Response {
    let url = proxy.endpoint(path);
    let mut request = proxy.client.request(method, url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some(body) = body {
        request = request.body(body);
    }

    let upstream = match request.send().await {
        Ok(response) => response,
        Err(err) => return upstream_error_response(&err),
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let mut response_headers = HeaderMap::new();
    for name in FORWARDED_RESPONSE_HEADERS {
        if let Some(value) = upstream.headers().get(name)
            && let Ok(value) = axum::http::HeaderValue::from_bytes(value.as_bytes())
        {
            response_headers.insert(name, value);
        }
    }

    match upstream.bytes().await {
        Ok(body) => (status, response_headers, body).into_response(),
        Err(err) => upstream_error_response(&err),
    }
}

/// Handles `POST /api/cellar/store` (cellar `POST /store`).
///
/// The JSON body is forwarded with `owner` / `session_id` overwritten by the
/// authenticated values, so the 210 / 211 / 212 handshake always runs against
/// the caller's own account.
async fn cellar_store_check_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CellarProxyQuery>,
    body: Bytes,
) -> Response {
    let Some(proxy) = state.cellar.as_ref() else {
        return proxy_not_configured_response();
    };

    let caller = match resolve_caller(&state, &headers, &query) {
        Ok(caller) => caller,
        Err(err) => return auth_error_response(err),
    };

    let mut payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(payload @ serde_json::Value::Object(_)) => payload,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "request body must be a JSON object" })),
            )
                .into_response();
        }
    };

    if let Some(map) = payload.as_object_mut() {
        map.insert(
            "owner".to_string(),
            serde_json::Value::from(caller.account_id),
        );
        map.insert(
            "session_id".to_string(),
            serde_json::Value::from(caller.session_id),
        );
    }

    let body = match serde_json::to_vec(&payload) {
        Ok(body) => body,
        Err(err) => {
            tracing::error!("failed to re-serialize cellar store request: {err}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid store request" })),
            )
                .into_response();
        }
    };

    let mut upstream_headers = owner_headers(&caller);
    upstream_headers.push(("Content-Type", "application/json".to_string()));

    forward_cellar_request(
        proxy,
        reqwest::Method::POST,
        "/store",
        upstream_headers,
        Some(Bytes::from(body)),
    )
    .await
}

/// Handles `POST|PUT /api/cellar/store/{tin_id}` (cellar `POST|PUT /store/{tin_id}`).
///
/// `X-Alhalte-Wrapped-Key` is forwarded as-is; ownership always comes from the
/// session, never from the request.
async fn cellar_store_upload_handler(
    State(state): State<AppState>,
    AxumPath(tin_id): AxumPath<String>,
    method: Method,
    headers: HeaderMap,
    Query(query): Query<CellarProxyQuery>,
    body: Bytes,
) -> Response {
    let Some(proxy) = state.cellar.as_ref() else {
        return proxy_not_configured_response();
    };

    let Some(tin_id) = normalize_tin_id(&tin_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "tin_id must be 64 hex characters" })),
        )
            .into_response();
    };

    let caller = match resolve_caller(&state, &headers, &query) {
        Ok(caller) => caller,
        Err(err) => return auth_error_response(err),
    };

    let mut upstream_headers = owner_headers(&caller);
    if let Some(wrapped_key) = headers
        .get("X-Alhalte-Wrapped-Key")
        .and_then(|v| v.to_str().ok())
    {
        upstream_headers.push(("X-Alhalte-Wrapped-Key", wrapped_key.to_string()));
    }
    if let Some(content_type) = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        upstream_headers.push(("Content-Type", content_type.to_string()));
    }

    let upstream_method = if method == Method::PUT {
        reqwest::Method::PUT
    } else {
        reqwest::Method::POST
    };

    forward_cellar_request(
        proxy,
        upstream_method,
        &format!("/store/{tin_id}"),
        upstream_headers,
        Some(body),
    )
    .await
}

/// Handles `GET /api/cellar/tin/{tin_id}` (cellar `GET /tin/{tin_id}`).
///
/// The raw tin bytes and the `X-Alhalte-Wrapped-Key` response header are
/// forwarded so `pantry` can decrypt with the room key.
async fn cellar_download_handler(
    State(state): State<AppState>,
    AxumPath(tin_id): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<CellarProxyQuery>,
) -> Response {
    let Some(proxy) = state.cellar.as_ref() else {
        return proxy_not_configured_response();
    };

    let Some(tin_id) = normalize_tin_id(&tin_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "tin_id must be 64 hex characters" })),
        )
            .into_response();
    };

    let caller = match resolve_caller(&state, &headers, &query) {
        Ok(caller) => caller,
        Err(err) => return auth_error_response(err),
    };

    forward_cellar_request(
        proxy,
        reqwest::Method::GET,
        &format!("/tin/{tin_id}"),
        owner_headers(&caller),
        None,
    )
    .await
}
