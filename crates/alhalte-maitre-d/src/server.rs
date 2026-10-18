//! HTTP and WebSocket gateway server routes and handlers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use alhalte_banquette::{ClientFrame, ServerFrame};

use crate::crypto;
use crate::db::{Account, Database, RoomRole};
use crate::gateway::RoomBridgeManager;
use crate::oauth::OAuthProvider;

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
    /// Client ID monotonic counter.
    client_counter: Arc<AtomicU64>,
}

impl AppState {
    /// Creates a new [`AppState`].
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
            client_counter: Arc::new(AtomicU64::new(1)),
        }
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
    /// Room identifier.
    pub room: Option<u64>,
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
        .route("/room/{room_id}/export/zip", get(export_room_zip_handler));

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

/// Handles `GET /api/room/{room_id}/permissions`.
async fn get_room_permissions_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<String>,
) -> Response {
    match state.db.get_room_permissions(&room_id) {
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
    AxumPath(room_id): AxumPath<String>,
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
    match state
        .db
        .set_room_permission(&room_id, req.account_id, req.role, now)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "room_id": room_id,
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
    AxumPath((room_id, account_id)): AxumPath<(String, u64)>,
) -> Response {
    match state.db.delete_room_permission(&room_id, account_id) {
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
            .and_then(|s| s.parse::<u64>().ok())
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
    AxumPath(room_id): AxumPath<u64>,
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
    room_id: u64,
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

    // Check room permissions and done flag
    let room_id_str = room_id.to_string();
    let has_permissions = match state.db.has_room_permissions(&room_id_str) {
        Ok(has) => has,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

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

    if has_permissions && user_role.is_none() {
        let is_done = state.room_manager.is_room_done(room_id).await;
        if !is_done {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください"
                })),
            )
                .into_response();
        }
    }

    let is_viewer = match user_role {
        Some(RoomRole::Viewer) => true,
        None if has_permissions => true,
        _ => false,
    };

    let client_id = state.next_client_id();
    let admin_account = match state.db.get_room_gm(&room_id_str) {
        Ok(Some(gm_id)) => gm_id,
        _ => {
            if auth_account != 0 {
                auth_account
            } else {
                1
            }
        }
    };

    let bridge = state
        .room_manager
        .get_or_create_room(room_id, admin_account)
        .await;

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
    room_id: u64,
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
                        state.room_manager.set_room_done(room_id, *done).await;
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
                            state.room_manager.set_room_done(room_id, *done).await;
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
    AxumPath(room_id): AxumPath<u64>,
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

    let admin_account = caller_account.unwrap_or(1);
    let bridge = state
        .room_manager
        .get_or_create_room(room_id, admin_account)
        .await;

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
