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

use alhalte_banquette::ClientFrame;

use crate::crypto;
use crate::db::{Account, Database};
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

/// Query parameters for session verification.
#[derive(Debug, Deserialize)]
pub struct VerifySessionQuery {
    /// Owning account ID (also accepts `account_id`).
    pub owner: Option<u64>,
    /// Owning account ID.
    pub account_id: Option<u64>,
    /// Session ID to verify.
    pub session_id: u64,
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
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    /// Session ID.
    pub session_id: u64,
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

    router.with_state(state)
}

/// Handles `GET /api/verify-session`.
///
/// Compatible with `alhalte-cellar`'s `MaitreDVerifier`.
async fn verify_session_handler(
    State(state): State<AppState>,
    Query(q): Query<VerifySessionQuery>,
) -> Response {
    let account_id = match q.owner.or(q.account_id) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing owner/account_id parameter" })),
            )
                .into_response();
        }
    };

    match state.db.is_session_valid(account_id, q.session_id) {
        Ok(true) => (
            StatusCode::OK,
            Json(VerifySessionResponse {
                valid: true,
                account_id,
                session_id: q.session_id,
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
    Json(req): Json<HeartbeatRequest>,
) -> Response {
    let now = chrono::Utc::now().timestamp();
    match state.db.record_heartbeat(req.session_id, now) {
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
    let session_id = req.session_id.or_else(|| {
        headers
            .get("X-Alhalte-Session")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
    });

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
    let account_id = query.account_id.or_else(|| {
        headers
            .get("X-Alhalte-Account")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
    });

    let session_id = query.session_id.or_else(|| {
        headers
            .get("X-Alhalte-Session")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
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
    }

    let auth_account = account_id.unwrap_or(0);
    let auth_session = session_id.unwrap_or(0);

    let client_id = state.next_client_id();
    let admin_account = if auth_account != 0 { auth_account } else { 1 };

    let bridge = state
        .room_manager
        .get_or_create_room(room_id, admin_account)
        .await;

    ws.on_upgrade(move |socket| {
        handle_websocket(socket, bridge, client_id, auth_account, auth_session)
    })
}

async fn handle_websocket(
    socket: WebSocket,
    bridge: Arc<crate::gateway::RoomBridge>,
    client_id: u64,
    account_id: u64,
    session_id: u64,
) {
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
                    bridge_ref.send_client_frame(client_id, frame);
                }
                Err(e) => {
                    tracing::warn!("Failed to deserialize ClientFrame from JSON: {e:?}");
                }
            },
            Message::Binary(bytes) => {
                // Try postcard first, then fallback to JSON
                let frame = postcard::from_bytes::<ClientFrame>(&bytes)
                    .or_else(|_| serde_json::from_slice::<ClientFrame>(&bytes));
                match frame {
                    Ok(f) => {
                        bridge_ref.send_client_frame(client_id, f);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to deserialize binary ClientFrame: {e:?}");
                    }
                }
            }
            Message::Ping(payload) => {
                // Axum handles pong automatically or we can ignore
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

    let caller_account = match (q.account_id, q.session_id) {
        (Some(account_id), Some(session_id)) => {
            match state.db.is_session_valid(account_id, session_id) {
                Ok(true) => Some(account_id),
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
            }
        }
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
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/zip"),
    );
    if let Ok(cd) =
        axum::http::HeaderValue::try_from(format!("attachment; filename=\"{filename}\""))
    {
        headers.insert(axum::http::header::CONTENT_DISPOSITION, cd);
    }

    (StatusCode::OK, headers, zip_bytes).into_response()
}
