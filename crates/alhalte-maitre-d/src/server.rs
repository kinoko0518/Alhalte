//! HTTP and WebSocket gateway server routes and handlers.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Form, Path as AxumPath, Query, State};
use axum::http::header::{
    ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, HeaderValue, LOCATION, SET_COOKIE,
};
use axum::http::{HeaderMap, HeaderName, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, CorsLayer};

use alhalte_banquette::{ClientFrame, RoomPermissionLevel};

use crate::auth::{
    AuthIdentity, AuthProvider, DebugAuthProvider, GoogleAuthProvider, MAX_DEBUG_ACCOUNT_NAME_LEN,
};
use crate::crypto;
use crate::db::{Account, Database, Room};
use crate::error::MaitreDError;
use crate::gateway::{RoomBridge, RoomBridgeManager};
use crate::oauth::OAuthProvider;
use crate::room_id::RoomId;

/// Serde helpers that carry 64-bit session IDs as decimal strings.
///
/// JavaScript numbers have 53 bits of integer precision, so a `session_id`
/// serialized as a JSON number above `Number.MAX_SAFE_INTEGER` is silently
/// rounded by `JSON.parse`. The browser would then store and send an ID that
/// matches no session row, making every authenticated request after sign-in fail
/// with 401 (Issue #220). Serializing the ID as a decimal string preserves all
/// 64 bits. Deserialization accepts both a string and a number so existing
/// numeric clients keep working.
mod session_id_json {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes a `u64` session ID as a decimal string.
    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    /// Deserializes a session ID from either a decimal string or a JSON number.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrNumber {
            String(String),
            Number(u64),
        }

        match StringOrNumber::deserialize(deserializer)? {
            StringOrNumber::String(value) => value.parse::<u64>().map_err(serde::de::Error::custom),
            StringOrNumber::Number(value) => Ok(value),
        }
    }

    /// Deserializes an optional session ID from either representation.
    pub fn deserialize_optional<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrNumber {
            String(String),
            Number(u64),
        }

        match Option::<StringOrNumber>::deserialize(deserializer)? {
            None => Ok(None),
            Some(StringOrNumber::String(value)) => value
                .parse::<u64>()
                .map(Some)
                .map_err(serde::de::Error::custom),
            Some(StringOrNumber::Number(value)) => Ok(Some(value)),
        }
    }
}

/// Default browser origins allowed by CORS in local development.
///
/// Production deployments should pass explicit `--cors-origin` values derived
/// from the restaurant's argenterie URL / domain. CORS never defaults to
/// `permissive()`.
pub const DEFAULT_ALLOWED_ORIGINS: [&str; 2] = ["http://127.0.0.1:5173", "http://localhost:5173"];

/// Default public base URL used to build the Google OAuth redirect URI.
///
/// Bistrot overrides this with `--public-url` derived from the restaurant
/// domain / argenterie URL. The redirect URI must stay fixed: Google rejects a
/// redirect URI that carries the argenterie origin or port.
pub const DEFAULT_PUBLIC_URL: &str = "http://127.0.0.1:8080";

/// Lifetime of the `session_id` cookie set by the OAuth callback (30 days).
///
/// The server-side session itself is still purged at 4:00 local time when it
/// has no recent heartbeat.
pub const SESSION_COOKIE_MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

/// Application shared state.
#[derive(Clone)]
pub struct AppState {
    /// Database manager.
    pub db: Database,
    /// Instance salt for Google `sub` encryption.
    pub salt: String,
    /// Authentication provider (Google OAuth or the isolated debug login).
    pub auth: Arc<dyn AuthProvider>,
    /// WebSocket room demuxer manager.
    pub room_manager: RoomBridgeManager,
    /// Optional authenticated reverse proxy to `alhalte-cellar` (`/api/cellar/*`).
    ///
    /// When `None`, the proxied routes answer `503 Service Unavailable`; no other
    /// route is affected.
    pub cellar: Option<CellarProxy>,
    /// Browser origins allowed by the CORS layer.
    ///
    /// Defaults to [`DEFAULT_ALLOWED_ORIGINS`]; an empty list disables
    /// cross-origin access entirely (same-origin only). The same allow-list
    /// gates OAuth `return_to` values.
    pub allowed_origins: Vec<String>,
    /// Development-only `http://<private IPv4>:<PORT>` origins (Issue #229).
    ///
    /// Empty by default. When non-empty (`--allow-lan-origin-port`), CORS and
    /// OAuth `return_to` also accept private-network IPv4 origins on these
    /// ports. This is not a wildcard: HTTPS, hostnames, and public addresses
    /// are still rejected, and the port must match exactly.
    pub lan_origin_ports: Vec<u16>,
    /// Public base URL used to build the fixed Google OAuth redirect URI.
    ///
    /// Defaults to [`DEFAULT_PUBLIC_URL`]; `--public-url` replaces it.
    pub public_url: String,
    /// Client ID monotonic counter.
    client_counter: Arc<AtomicU64>,
}

impl AppState {
    /// Creates a new [`AppState`] without a cellar proxy.
    ///
    /// `oauth` is wrapped as the normal Google login flow
    /// ([`GoogleAuthProvider`]). Use [`AppState::with_debug_login`] to replace the
    /// whole authentication path with the isolated debug login,
    /// [`AppState::with_cellar_proxy`] (or [`AppState::with_cellar_url`]) to
    /// enable the `/api/cellar/*` routes,
    /// [`AppState::with_allowed_origins`] to replace the local-development CORS
    /// allow-list, and [`AppState::with_public_url`] to replace the OAuth
    /// redirect base.
    pub fn new(
        db: Database,
        salt: impl Into<String>,
        oauth: Arc<dyn OAuthProvider>,
        room_manager: RoomBridgeManager,
    ) -> Self {
        Self {
            db,
            salt: salt.into(),
            auth: Arc::new(GoogleAuthProvider::new(oauth)),
            room_manager,
            cellar: None,
            allowed_origins: DEFAULT_ALLOWED_ORIGINS
                .iter()
                .map(|origin| (*origin).to_string())
                .collect(),
            lan_origin_ports: Vec::new(),
            public_url: DEFAULT_PUBLIC_URL.to_string(),
            client_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Switches the authentication path to the isolated debug login (Issue #150).
    ///
    /// [`create_router`] then registers `GET` / `POST /auth/debug/login`, drops
    /// the Google callback / token routes, `GET /auth/login` redirects to the
    /// debug form, and `GET /api/server-info` reports `debug_login: true`.
    #[must_use]
    pub fn with_debug_login(mut self) -> Self {
        self.auth = Arc::new(DebugAuthProvider::new());
        self
    }

    /// Replaces the authentication provider (tests / embedders).
    #[must_use]
    pub fn with_auth_provider(mut self, auth: Arc<dyn AuthProvider>) -> Self {
        self.auth = auth;
        self
    }

    /// Returns true when the debug login flow is active.
    pub fn is_debug_login(&self) -> bool {
        self.auth.kind().is_debug()
    }

    /// Replaces the CORS origin allow-list.
    ///
    /// An empty iterator disables cross-origin access (same-origin requests keep
    /// working). Invalid origins are ignored when the layer is built. The same
    /// list is used to validate OAuth `return_to` origins.
    #[must_use]
    pub fn with_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_origins = origins.into_iter().map(Into::into).collect();
        self
    }

    /// Enables the development-only LAN origin switch (Issue #229).
    ///
    /// Each port lets CORS and OAuth `return_to` accept
    /// `http://<private IPv4>:<port>` in addition to [`Self::allowed_origins`].
    /// This never makes CORS permissive: the scheme, host range, and port are
    /// checked. Use only for same-Ethernet testing, never on a public instance.
    #[must_use]
    pub fn with_lan_origin_ports(mut self, ports: impl IntoIterator<Item = u16>) -> Self {
        self.lan_origin_ports = ports.into_iter().collect();
        self
    }

    /// Replaces the public base URL used for the Google OAuth redirect URI.
    #[must_use]
    pub fn with_public_url(mut self, public_url: impl Into<String>) -> Self {
        self.public_url = public_url.into();
        self
    }

    /// Returns the fixed Google OAuth redirect URI (`{public_url}/auth/google/callback`).
    ///
    /// The argenterie origin / port is never part of this URL; it is the value
    /// registered in Google Cloud Console.
    pub fn oauth_redirect_uri(&self) -> String {
        format!(
            "{}/auth/google/callback",
            self.public_url.trim().trim_end_matches('/')
        )
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
    /// Session ID, as a decimal string so browsers keep all 64 bits.
    #[serde(with = "session_id_json")]
    pub session_id: u64,
}

/// Request payload for heartbeat.
#[derive(Debug, Deserialize, Default)]
pub struct HeartbeatRequest {
    /// Session ID (decimal string or legacy JSON number).
    #[serde(default, deserialize_with = "session_id_json::deserialize_optional")]
    pub session_id: Option<u64>,
}

/// Query parameters for searching accounts.
#[derive(Debug, Deserialize, Default)]
pub struct SearchAccountsQuery {
    /// Search keyword (handle name or numeric account ID).
    pub q: Option<String>,
    /// Optional claimed account ID (must agree with the session).
    pub account_id: Option<u64>,
    /// Optional session ID; a cookie / `X-Alhalte-Session` header also works.
    pub session_id: Option<u64>,
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

/// Public account profile returned by `GET /api/account/{account_id}`.
///
/// This is a response DTO: the stored [`Account::sub_hash`] (the salted 128-bit
/// hash of the Google identifier) is never serialized to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountResponse {
    /// 64-bit account ID.
    pub account_id: u64,
    /// Handle name / nickname.
    pub handle_name: String,
    /// Optional 32-byte Tin ID pointing to the user's avatar icon.
    pub icon_tin_id: Option<[u8; 32]>,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at: i64,
}

impl From<Account> for AccountResponse {
    fn from(account: Account) -> Self {
        Self {
            account_id: account.account_id,
            handle_name: account.handle_name,
            icon_tin_id: account.icon_tin_id,
            created_at: account.created_at,
        }
    }
}

/// Room role as exposed by the maitre-d HTTP API.
///
/// The names mirror `alhalte-banquette`'s four permission levels. This is a
/// transport DTO only: `maitre-d` stores no permission of its own and answers
/// every access question by asking the room's banquette
/// (`QueryAccountPermission` / `QueryPermissions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomRole {
    /// Full access, including the room permission table.
    Admin,
    /// Read/write the scenes an administrator moved the account into.
    Participant,
    /// Read-only access to every scene.
    Viewer,
    /// No access to the room.
    Denied,
}

impl RoomRole {
    /// Returns the JSON / query-string name of this role.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Participant => "participant",
            Self::Viewer => "viewer",
            Self::Denied => "denied",
        }
    }
}

impl From<RoomPermissionLevel> for RoomRole {
    fn from(level: RoomPermissionLevel) -> Self {
        match level {
            RoomPermissionLevel::Admin => Self::Admin,
            RoomPermissionLevel::Participant => Self::Participant,
            RoomPermissionLevel::Viewer => Self::Viewer,
            RoomPermissionLevel::Denied => Self::Denied,
        }
    }
}

impl std::fmt::Display for RoomRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// One entry of the room permission table returned by `GET /api/room/{id}/permissions`.
///
/// `username` / `icon_url` are joined from the maitre-d `accounts` table; the
/// permission itself comes from banquette's `RoomData`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPermissionItem {
    /// 64-bit account ID.
    pub account_id: u64,
    /// User name / handle name.
    pub username: String,
    /// Avatar icon URL if available.
    pub icon_url: Option<String>,
    /// Effective role in the room.
    pub role: RoomRole,
}

/// Request payload for setting room permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRoomPermissionRequest {
    /// Account ID to assign permission.
    pub account_id: u64,
    /// Assigned role in the room.
    pub role: RoomRole,
}

/// Query parameters for `GET /auth/login`.
#[derive(Debug, Default, Deserialize)]
pub struct OAuthLoginQuery {
    /// Absolute URL to return to after Google authentication.
    ///
    /// Only origins in the CORS allow-list are accepted; the browser cannot
    /// send a fragment, so argenterie restores it from `sessionStorage`.
    pub return_to: Option<String>,
    /// Optional Google `prompt` value (`select_account` / `consent` / `none`).
    pub prompt: Option<String>,
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
///
/// The account is served as [`AccountResponse`]: neither the raw Google `sub`
/// nor its stored hash is exposed.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Public account profile.
    pub account: AccountResponse,
    /// Issued 64-bit session ID, as a decimal string so browsers keep all 64 bits.
    #[serde(with = "session_id_json")]
    pub session_id: u64,
}

/// Response payload of `GET /api/session`.
///
/// The callback sets the `session_id` cookie; argenterie calls this endpoint
/// after the redirect to learn the account ID and persist the session in the
/// same places the room socket reads. `sub_hash` is never exposed.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionResponse {
    /// Public account profile for the session owner.
    pub account: AccountResponse,
    /// Session ID resolved from the cookie / header / query, as a decimal string
    /// so browsers keep all 64 bits.
    #[serde(with = "session_id_json")]
    pub session_id: u64,
}

/// Request payload for profile update.
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    /// Session ID for authorization (decimal string or legacy JSON number).
    #[serde(default, deserialize_with = "session_id_json::deserialize_optional")]
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

/// Response payload of `GET /api/server-info`.
///
/// Public on purpose: argenterie reads it before sign-in and shows the
/// full-screen debug-login warning when `debug_login` is true (Issue #150).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfoResponse {
    /// True when this `maitre-d` runs with `--debug-login`.
    pub debug_login: bool,
}

/// Query parameters of `GET /auth/debug/login`.
#[derive(Debug, Default, Deserialize)]
pub struct DebugLoginQuery {
    /// Absolute URL to return to after the debug login.
    ///
    /// Only origins in the CORS allow-list are accepted, exactly like
    /// `GET /auth/login`.
    pub return_to: Option<String>,
}

/// Form payload of `POST /auth/debug/login`.
#[derive(Debug, Deserialize)]
pub struct DebugLoginForm {
    /// Absolute URL to return to after the debug login.
    pub return_to: Option<String>,
    /// Account name. A name that does not exist yet creates an account.
    pub account_name: String,
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
    /// Caller role in the room (`admin`, `participant`, `viewer`, or `denied`).
    ///
    /// `denied` is never returned: a caller without access gets `403` instead.
    pub role: RoomRole,
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
///
/// With the debug login active (`AppState::with_debug_login`) the Google
/// callback / token routes are not registered at all and `/auth/debug/login` is;
/// without it `/auth/debug/login` answers `404 Not Found` (Issue #150).
pub fn create_router(state: AppState) -> Router {
    let debug_login = state.is_debug_login();
    let mut router = Router::new()
        .route("/api/verify-session", get(verify_session_handler))
        .route("/api/session", get(session_handler))
        .route("/api/heartbeat", post(heartbeat_handler))
        // Public server metadata: argenterie reads `debug_login` before sign-in.
        .route("/api/server-info", get(server_info_handler))
        // OAuth / debug login entry point: maitre-d owns the state and the fixed
        // redirect URI, or hands the browser to the debug form.
        .route("/auth/login", get(oauth_login_handler))
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

    if debug_login {
        // Issue #150: the debug login is a separate path, not a branch inside
        // the Google flow. The Google routes do not exist while it is active.
        router = router.route(
            "/auth/debug/login",
            get(debug_login_form_handler).post(debug_login_submit_handler),
        );
    } else {
        router = router
            .route("/auth/google/callback", get(oauth_callback_handler))
            .route("/auth/google/token", post(oauth_token_handler));
    }

    #[cfg(debug_assertions)]
    {
        router = router.route("/api/debug/account", post(create_debug_account_handler));
    }

    router
        .layer(build_cors_layer(
            &state.allowed_origins,
            &state.lan_origin_ports,
        ))
        .with_state(state)
}

/// Builds the CORS layer from an explicit origin allow-list.
///
/// Credentials (session cookies / `X-alhalte-*` headers) are allowed, so the
/// origin list must be explicit: [`CorsLayer::permissive`] is never used. An
/// empty list keeps the layer same-origin only.
///
/// `lan_origin_ports` is the development-only Issue #229 switch: when non-empty
/// it additionally accepts `http://<private IPv4>:<port>` through the same
/// predicate as [`is_allowed_return_to_with_lan`], never a wildcard.
fn build_cors_layer(origins: &[String], lan_origin_ports: &[u16]) -> CorsLayer {
    // `allow_credentials` is required for the session cookie, and browsers
    // reject `Access-Control-Allow-Headers: *` together with credentials, so
    // the headers the gateways actually use are listed explicitly.
    let allowed_headers = [
        ACCEPT,
        CONTENT_TYPE,
        AUTHORIZATION,
        HeaderName::from_static("x-alhalte-account"),
        HeaderName::from_static("x-alhalte-session"),
        HeaderName::from_static("x-alhalte-room"),
        HeaderName::from_static("x-alhalte-wrapped-key"),
        HeaderName::from_static("x-alhalte-service-token"),
    ];
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(allowed_headers)
        .allow_credentials(true);

    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| origin.trim().parse::<HeaderValue>().ok())
        .collect();

    if parsed.is_empty() && lan_origin_ports.is_empty() {
        return layer;
    }

    // `AllowOrigin::predicate` echoes the request origin only when it matches,
    // which keeps `allow_credentials` valid without ever sending `*`. Normalize
    // both sides so `http://host:80` and `http://host` compare equal.
    let allowed: Vec<String> = parsed
        .iter()
        .filter_map(|origin| origin.to_str().ok())
        .map(str::to_string)
        .collect();
    let lan_ports = lan_origin_ports.to_vec();
    layer.allow_origin(AllowOrigin::predicate(
        move |origin: &HeaderValue, _parts: &axum::http::request::Parts| {
            origin
                .to_str()
                .is_ok_and(|origin| is_allowed_origin(origin, &allowed, &lan_ports))
        },
    ))
}

/// Handles `GET /api/server-info`.
///
/// Public and unauthenticated: argenterie asks for it at startup and shows the
/// full-screen warning when `debug_login` is true (Issue #150).
async fn server_info_handler(State(state): State<AppState>) -> Json<ServerInfoResponse> {
    Json(ServerInfoResponse {
        debug_login: state.is_debug_login(),
    })
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

/// Builds an error response with the shared JSON error shape.
fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// Returns a `302 Found` response that sends the browser to `location`.
fn redirect_response(location: &str) -> Response {
    match HeaderValue::from_str(location) {
        Ok(value) => {
            let mut response = StatusCode::FOUND.into_response();
            response.headers_mut().insert(LOCATION, value);
            response
        }
        Err(_) => error_response(StatusCode::BAD_REQUEST, "Invalid redirect target"),
    }
}

/// Builds the `Set-Cookie` value for an authenticated browser session.
///
/// `SameSite=Lax` covers the top-level OAuth callback redirect. `Secure` is
/// added when the public URL is HTTPS.
pub fn session_cookie_header(session_id: u64, secure: bool, max_age_secs: i64) -> String {
    let mut cookie =
        format!("session_id={session_id}; Path=/; Max-Age={max_age_secs}; SameSite=Lax");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Returns the normalized `scheme://host[:port]` origin of an absolute HTTP(S) URL.
///
/// Fragments are ignored because `http::Uri` has no fragment component; query
/// strings do not affect the origin. Relative URLs, non-HTTP schemes, and URLs
/// with userinfo are rejected.
fn normalized_origin(value: &str) -> Option<String> {
    let without_fragment = value.split('#').next().unwrap_or(value);
    let uri: http::Uri = without_fragment.parse().ok()?;
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = uri.authority()?;
    // `http://allowed.example@evil.example` must not inherit the allowed origin.
    if authority.as_str().contains('@') {
        return None;
    }
    let host = authority.host().to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let default_port = if scheme == "http" { 80 } else { 443 };
    match authority.port_u16().filter(|port| *port != default_port) {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

/// Returns true when `return_to` is an absolute URL on an allowed origin.
///
/// The allow-list is the same explicit origin list used for CORS. This is the
/// open-redirect guard for `GET /auth/login`: `//evil.example`, another origin,
/// and `javascript:` / `data:` URLs are all rejected.
///
/// This is the production-safe form (no LAN switch). The HTTP handlers call
/// [`is_allowed_return_to_with_lan`] with
/// [`AppState::lan_origin_ports`].
pub fn is_allowed_return_to(return_to: &str, allowed_origins: &[String]) -> bool {
    is_allowed_return_to_with_lan(return_to, allowed_origins, &[])
}

/// Returns true when `value` is an allowed browser origin.
///
/// The explicit `allowed_origins` list is always honored. Additionally, when
/// `allow_lan_origin_ports` is non-empty (the development-only
/// `--allow-lan-origin-port` switch, Issue #229), an `http://` origin whose
/// host is a private-network IPv4 literal and whose port is listed is accepted.
///
/// The extra rule intentionally rejects:
/// - non-HTTP(S) schemes (already rejected by [`normalized_origin`]),
/// - `https://` private origins (the Vite dev server is plain HTTP),
/// - hostnames (`my-host.local`, `192.168.1.1.evil.example`) and public IPs,
/// - any other port.
pub fn is_allowed_return_to_with_lan(
    value: &str,
    allowed_origins: &[String],
    allow_lan_origin_ports: &[u16],
) -> bool {
    is_allowed_origin(value, allowed_origins, allow_lan_origin_ports)
}

/// Shared by the CORS predicate and the `return_to` guard.
fn is_allowed_origin(value: &str, allowed_origins: &[String], lan_origin_ports: &[u16]) -> bool {
    let Some(origin) = normalized_origin(value) else {
        return false;
    };
    if allowed_origins
        .iter()
        .any(|allowed| normalized_origin(allowed).as_deref() == Some(origin.as_str()))
    {
        return true;
    }
    is_allowed_lan_origin(&origin, lan_origin_ports)
}

/// Returns true for `http://<private IPv4>:<listed port>` origins.
///
/// `origin` must already be normalized by [`normalized_origin`]. Only RFC 1918
/// ranges and loopback are accepted, so `--allow-lan-origin-port` cannot turn
/// into a public wildcard.
fn is_allowed_lan_origin(normalized: &str, lan_origin_ports: &[u16]) -> bool {
    if lan_origin_ports.is_empty() {
        return false;
    }
    let Ok(uri) = normalized.parse::<http::Uri>() else {
        return false;
    };
    if uri.scheme_str() != Some("http") {
        return false;
    }
    let Some(authority) = uri.authority() else {
        return false;
    };
    let Ok(ip) = authority.host().parse::<Ipv4Addr>() else {
        return false;
    };
    let port = authority.port_u16().unwrap_or(80);
    is_private_network_ipv4(ip) && lan_origin_ports.contains(&port)
}

/// Returns true for private-network / loopback IPv4 literals.
fn is_private_network_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback()
}

/// Returns true for the Google `prompt` values argenterie may request.
fn is_allowed_prompt(prompt: Option<&str>) -> bool {
    matches!(prompt, Some("select_account" | "consent" | "none"))
}

/// Creates a random, URL-safe OAuth state (32 bytes, hex encoded).
fn generate_oauth_state() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

/// Handles `GET /auth/login`.
///
/// The browser navigates here with an absolute `return_to`, which is validated
/// against the CORS origin allow-list first (open-redirect guard). The
/// [`AuthProvider`] then decides the next step: the debug login redirects to
/// `/auth/debug/login`, while Google issues a single-use state, binds it to
/// `return_to`, and redirects to Google with the fixed
/// `{public_url}/auth/google/callback` redirect URI (the argenterie origin /
/// port is never sent to Google).
async fn oauth_login_handler(
    State(state): State<AppState>,
    Query(q): Query<OAuthLoginQuery>,
) -> Response {
    let Some(return_to) = q
        .return_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return error_response(StatusCode::BAD_REQUEST, "Missing return_to");
    };

    if !is_allowed_return_to_with_lan(return_to, &state.allowed_origins, &state.lan_origin_ports) {
        return error_response(StatusCode::BAD_REQUEST, "return_to origin is not allowed");
    }

    // Debug login: the provider owns the next step (a local form).
    if let Some(target) = state.auth.login_redirect(return_to) {
        return redirect_response(&target);
    }

    let now = chrono::Utc::now().timestamp();
    let oauth_state = generate_oauth_state();
    if let Err(error) = state.db.create_oauth_state(
        &oauth_state,
        return_to,
        now,
        crate::db::OAUTH_STATE_TTL_SECS,
    ) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }

    let redirect_uri = state.oauth_redirect_uri();
    let Some(mut url) = state.auth.authorization_url(&redirect_uri, &oauth_state) else {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Authentication provider cannot start a Google login",
        );
    };
    if let Some(prompt) = q.prompt.as_deref().filter(|p| is_allowed_prompt(Some(p))) {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str("prompt=");
        url.push_str(prompt);
    }

    redirect_response(&url)
}

/// Handles `GET /auth/google/callback`.
///
/// Google calls this fixed URL. The state is consumed (single-use, 10 minute
/// TTL), the code is exchanged, the account / session are created, and the
/// browser is sent back to the `return_to` bound to the state with a
/// `session_id` cookie.
async fn oauth_callback_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if let Some(error) = params.get("error") {
        return error_response(
            StatusCode::BAD_REQUEST,
            format!("Google OAuth error: {error}"),
        );
    }

    let Some(code) = params.get("code").map(String::as_str) else {
        return error_response(StatusCode::BAD_REQUEST, "Missing code");
    };
    let Some(oauth_state) = params.get("state").map(String::as_str) else {
        return error_response(StatusCode::BAD_REQUEST, "Missing state");
    };

    let now = chrono::Utc::now().timestamp();
    let return_to =
        match state
            .db
            .consume_oauth_state(oauth_state, now, crate::db::OAUTH_STATE_TTL_SECS)
        {
            Ok(Some(value)) => value,
            Ok(None) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "Unknown, expired, or already used OAuth state",
                );
            }
            Err(error) => {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
            }
        };

    // Defense in depth: the allow-list may have changed between login and callback.
    if !is_allowed_return_to_with_lan(&return_to, &state.allowed_origins, &state.lan_origin_ports) {
        return error_response(StatusCode::BAD_REQUEST, "return_to origin is not allowed");
    }

    let redirect_uri = state.oauth_redirect_uri();
    let identity = match state.auth.exchange_code(code, &redirect_uri).await {
        Ok(identity) => identity,
        Err(error) => return error_response(StatusCode::UNAUTHORIZED, error.to_string()),
    };

    let (_account, session) = match issue_account_session(&state, identity, now) {
        Ok(values) => values,
        Err((status, message)) => return error_response(status, message),
    };

    let secure = state.public_url.trim().starts_with("https://");
    match with_session_cookie(redirect_response(&return_to), session.session_id, secure) {
        Some(response) => response,
        None => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to build session cookie",
        ),
    }
}

/// Looks up or creates the account for a provider identity and issues a session.
///
/// The identity's subject (a Google `sub`, or the synthetic debug key) is hashed
/// and zeroized in place, so the raw identifier is never written to the
/// database, a log line, or a response.
fn issue_account_session(
    state: &AppState,
    identity: AuthIdentity,
    now: i64,
) -> std::result::Result<(Account, crate::db::Session), (StatusCode, String)> {
    let (sub_hash, handle_name) = identity.into_sub_hash(&state.salt);
    let account = state
        .db
        .get_or_create_account(&sub_hash, &handle_name, None, now)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let session = state
        .db
        .create_session(account.account_id, now)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok((account, session))
}

/// Appends the `session_id` `Set-Cookie` header to `response`.
///
/// Returns `None` when the header value cannot be built, so the caller answers
/// with a 500 instead of silently dropping the cookie.
fn with_session_cookie(mut response: Response, session_id: u64, secure: bool) -> Option<Response> {
    let cookie = session_cookie_header(session_id, secure, SESSION_COOKIE_MAX_AGE_SECS);
    let value = HeaderValue::from_str(&cookie).ok()?;
    response.headers_mut().append(SET_COOKIE, value);
    Some(response)
}

/// Handles `GET /api/session`.
///
/// Resolves the caller from the `session_id` cookie / header / query and
/// returns the public account profile plus the session ID. argenterie calls
/// this after the OAuth redirect to store the account and session in the same
/// places the room socket reads.
async fn session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RoomSessionQuery>,
) -> Response {
    let session_id = query
        .session_id
        .or_else(|| {
            headers
                .get("X-Alhalte-Session")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .or_else(|| extract_session_id_from_cookie(&headers));

    let Some(session_id) = session_id else {
        return error_response(StatusCode::UNAUTHORIZED, "Missing session ID");
    };

    let session = match state.db.get_session(session_id) {
        Ok(Some(session)) => session,
        Ok(None) => return error_response(StatusCode::UNAUTHORIZED, "Invalid session"),
        Err(error) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
        }
    };

    let account = match state.db.get_account(session.account_id) {
        Ok(Some(account)) => account,
        Ok(None) => return error_response(StatusCode::UNAUTHORIZED, "Invalid session"),
        Err(error) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
        }
    };

    Json(SessionResponse {
        account: AccountResponse::from(account),
        session_id,
    })
    .into_response()
}

/// Handles `POST /auth/google/token`.
async fn oauth_token_handler(
    State(state): State<AppState>,
    Json(req): Json<OAuthTokenRequest>,
) -> Response {
    let identity = match state.auth.verify_id_token(&req.id_token).await {
        Ok(identity) => identity,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // The caller may suggest the default handle name for a new account.
    let identity = match req.handle_name.as_deref() {
        Some(name) => identity.with_handle_name(name),
        None => identity,
    };

    let now = chrono::Utc::now().timestamp();
    let (account, session) = match issue_account_session(&state, identity, now) {
        Ok(values) => values,
        Err((status, message)) => return error_response(status, message),
    };

    Json(AuthResponse {
        account: AccountResponse::from(account),
        session_id: session.session_id,
    })
    .into_response()
}

/// Validates the `return_to` of a debug login request.
///
/// The same rules as `GET /auth/login` apply: an absolute URL on a CORS-allowed
/// origin, never a protocol-relative or foreign URL.
#[allow(clippy::result_large_err)]
fn debug_return_to<'a>(
    state: &AppState,
    raw: Option<&'a str>,
) -> std::result::Result<&'a str, Response> {
    let Some(return_to) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(error_response(StatusCode::BAD_REQUEST, "Missing return_to"));
    };
    if !is_allowed_return_to_with_lan(return_to, &state.allowed_origins, &state.lan_origin_ports) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "return_to origin is not allowed",
        ));
    }
    Ok(return_to)
}

/// Escapes a value for inclusion in HTML text / a double-quoted attribute.
fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Handles `GET /auth/debug/login` (debug login only).
///
/// Answers a minimal HTML form asking for an account name; the only other input
/// is the `return_to` the browser is sent back to. The route exists only when the
/// debug login is active, so it is `404 Not Found` otherwise.
async fn debug_login_form_handler(
    State(state): State<AppState>,
    Query(q): Query<DebugLoginQuery>,
) -> Response {
    let Ok(return_to) = debug_return_to(&state, q.return_to.as_deref()) else {
        return error_response(StatusCode::BAD_REQUEST, "Missing or disallowed return_to");
    };

    let return_to_value = html_escape(return_to);
    let max_len = MAX_DEBUG_ACCOUNT_NAME_LEN;
    let body = format!(
        "<!doctype html>
<html lang=\"ja\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>デバッグログイン</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 0; padding: 2rem; background: #1c1b1f; color: #e6e1e5; }}
main {{ max-width: 24rem; margin: 4rem auto; display: flex; flex-direction: column; gap: 1rem; }}
h1 {{ font-size: 1.5rem; margin: 0; }}
p {{ margin: 0; line-height: 1.6; }}
input {{ font: inherit; padding: 0.75rem; border-radius: 0.5rem; border: 1px solid #938f99; background: #2b2930; color: inherit; }}
button {{ font: inherit; padding: 0.75rem; border-radius: 0.5rem; border: 0; background: #d0bcff; color: #381e72; cursor: pointer; }}
</style>
</head>
<body>
<main>
<h1>デバッグログイン</h1>
<p>アカウント名を入力してください。存在しない名前は、その名前で新しいアカウントとして作成されます。</p>
<form method=\"post\" action=\"/auth/debug/login\">
<input type=\"hidden\" name=\"return_to\" value=\"{return_to_value}\">
<label for=\"account-name\">アカウント名</label>
<input id=\"account-name\" name=\"account_name\" type=\"text\" required maxlength=\"{max_len}\" autocomplete=\"off\" autofocus>
<button type=\"submit\">ログイン</button>
</form>
</main>
</body>
</html>"
    );

    (
        [
            (CONTENT_TYPE, "text/html; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

/// Handles `POST /auth/debug/login` (debug login only).
///
/// The trimmed account name is the whole credential: an unknown name creates an
/// account, a known name reuses it. The subject is derived from the name and
/// hashed with the instance salt, then a session is issued and the browser is
/// sent back to `return_to` with the `session_id` cookie.
async fn debug_login_submit_handler(
    State(state): State<AppState>,
    Form(form): Form<DebugLoginForm>,
) -> Response {
    if !state.is_debug_login() {
        return error_response(StatusCode::NOT_FOUND, "Debug login is not enabled");
    }

    let return_to = match debug_return_to(&state, form.return_to.as_deref()) {
        Ok(value) => value.to_string(),
        Err(response) => return response,
    };

    let Some(identity) = state.auth.debug_identity(&form.account_name) else {
        return error_response(
            StatusCode::BAD_REQUEST,
            format!(
                "account_name must be 1..={MAX_DEBUG_ACCOUNT_NAME_LEN} characters without control \
                 characters"
            ),
        );
    };

    let now = chrono::Utc::now().timestamp();
    let (_account, session) = match issue_account_session(&state, identity, now) {
        Ok(values) => values,
        Err((status, message)) => return error_response(status, message),
    };

    let secure = state.public_url.trim().starts_with("https://");
    match with_session_cookie(redirect_response(&return_to), session.session_id, secure) {
        Some(response) => response,
        None => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to build session cookie",
        ),
    }
}

/// Handles `GET /api/account/{account_id}`.
///
/// Requires a valid session. The response is [`AccountResponse`], which omits
/// the stored `sub_hash`.
async fn get_account_handler(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<u64>,
    headers: HeaderMap,
    Query(query): Query<RoomSessionQuery>,
) -> Response {
    if let Err(response) =
        resolve_authenticated_account(&state, &headers, query.account_id, query.session_id)
    {
        return response.into_response();
    }

    match state.db.get_account(account_id) {
        Ok(Some(account)) => Json(AccountResponse::from(account)).into_response(),
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
///
/// Creates a throwaway account keyed by an unguessable synthetic subject and
/// issues a session for it. The response is an [`AuthResponse`], so the stored
/// hash never leaves the server.
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

    // A debug account used to be the literal `sub = "0"`. With the ciphertext
    // column gone (#190), each debug account is keyed by an unguessable
    // synthetic subject instead, so the unique `sub_hash` constraint holds and
    // a debug account is indistinguishable from a real one in the database.
    // Issue #150 moves debug accounts to a dedicated database.
    let mut debug_sub = format!("alhalte/debug/{}", hex::encode(rand::random::<[u8; 32]>()));
    let sub_hash = crypto::hash_sub(&state.salt, &mut debug_sub);

    let account = match state
        .db
        .get_or_create_account(&sub_hash, &req.handle_name, icon_bytes, now)
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
        account: AccountResponse::from(account),
        session_id: session.session_id,
    })
    .into_response()
}

/// Handles `GET /api/accounts/search?q={query}`.
///
/// Requires a valid session; anonymous account enumeration is rejected.
async fn search_accounts_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchAccountsQuery>,
) -> Response {
    if let Err(response) =
        resolve_authenticated_account(&state, &headers, query.account_id, query.session_id)
    {
        return response.into_response();
    }

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
///
/// The creator becomes the room's `Admin` when banquette first starts the room
/// (`Room::new` in `alhalte-banquette`); `maitre-d` writes no permission row.
fn room_created_response(room: Room) -> Response {
    (
        StatusCode::OK,
        Json(RoomInfoResponse {
            room_id: room.room_id,
            name: room.name,
            role: RoomRole::Admin,
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

/// Error message returned when the caller has no access to a room.
const ROOM_ACCESS_DENIED_MESSAGE: &str =
    "全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください";

/// Error message returned when an anonymous caller has no access to a room.
const ANONYMOUS_ROOM_ACCESS_DENIED_MESSAGE: &str = "匿名接続は全体公開された部屋でのみ許可されます";

/// Resolves a registered room and its live bridge, spawning banquette if needed.
///
/// An unknown room is `404` and never starts a process. The bridge
/// administrator is always `rooms.creator_account`; the caller neither picks it
/// nor influences it (Issue #200).
#[allow(clippy::result_large_err)]
async fn resolve_room_bridge(
    state: &AppState,
    room_id: &RoomId,
) -> std::result::Result<(Room, Arc<RoomBridge>), Response> {
    let room_id_str = room_id.to_string();
    let room = match state.db.get_room(&room_id_str) {
        Ok(Some(room)) => room,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Room not found" })),
            )
                .into_response());
        }
        Err(error) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response());
        }
    };

    match state
        .room_manager
        .get_or_create_room(room_id.as_u64(), room.creator_account)
        .await
    {
        Ok(bridge) => Ok((room, bridge)),
        Err(error) => {
            tracing::error!("Failed to start banquette for room {room_id_str}: {error}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to restore the room process: {error}")
                })),
            )
                .into_response())
        }
    }
}

/// Asks the room's banquette for the effective role of `account_id`.
///
/// `account_id == 0` asks for the room default, i.e. what an anonymous caller
/// gets. The answer is banquette's `RoomData`; `maitre-d` keeps no role table.
#[allow(clippy::result_large_err)]
async fn resolve_room_role(
    bridge: &RoomBridge,
    account_id: u64,
) -> std::result::Result<RoomRole, Response> {
    match bridge.query_account_permission(account_id).await {
        Ok(level) => Ok(RoomRole::from(level)),
        Err(error) => {
            tracing::error!(
                "Failed to query the permission of account {account_id} in room {}: {error}",
                bridge.room_id
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to read the room permission: {error}")
                })),
            )
                .into_response())
        }
    }
}

/// Rejects a caller whose banquette role is `Denied`.
#[allow(clippy::result_large_err)]
fn require_room_access(role: RoomRole, anonymous: bool) -> std::result::Result<(), Response> {
    if role == RoomRole::Denied {
        let message = if anonymous {
            ANONYMOUS_ROOM_ACCESS_DENIED_MESSAGE
        } else {
            ROOM_ACCESS_DENIED_MESSAGE
        };
        return Err(error_response(StatusCode::FORBIDDEN, message));
    }
    Ok(())
}

/// Maps a failed banquette permission operation to an HTTP response.
///
/// banquette refuses a permission change with an `Error` frame (not an
/// administrator, last administrator demotion); that is a `403`. Everything else
/// is a room-process failure and stays a `500`.
fn permission_error_response(error: MaitreDError) -> Response {
    match error {
        MaitreDError::RoomPermissionRefused(message) => {
            error_response(StatusCode::FORBIDDEN, message)
        }
        other => {
            tracing::error!("banquette permission request failed: {other}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to apply the room permission: {other}"),
            )
        }
    }
}

/// Handles `GET /api/rooms/{room_id}`.
///
/// Requires a session. The room process is started on first use and its
/// banquette is asked for the caller's role: `denied` is `403`, otherwise
/// `200` with the four-level role.
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

    let (room, bridge) = match resolve_room_bridge(&state, &room_id).await {
        Ok(values) => values,
        Err(response) => return response,
    };

    let role = match resolve_room_role(&bridge, caller).await {
        Ok(role) => role,
        Err(response) => return response,
    };
    if let Err(response) = require_room_access(role, false) {
        return response;
    }

    Json(RoomInfoResponse {
        room_id: room.room_id,
        name: room.name,
        role,
    })
    .into_response()
}

/// Handles `GET /api/room/{room_id}/permissions`.
///
/// Requires a session and room access. The table is banquette's
/// `RoomData.permissions` (read through `QueryPermissions`), joined with the
/// maitre-d account profile for display. `maitre-d` never answers it from its
/// own database.
async fn get_room_permissions_handler(
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

    let (_room, bridge) = match resolve_room_bridge(&state, &room_id).await {
        Ok(values) => values,
        Err(response) => return response,
    };

    let role = match resolve_room_role(&bridge, caller).await {
        Ok(role) => role,
        Err(response) => return response,
    };
    if let Err(response) = require_room_access(role, false) {
        return response;
    }

    let permissions = match bridge.permissions_snapshot(caller).await {
        Ok(Some((_default, permissions))) => permissions,
        Ok(None) => {
            return error_response(StatusCode::FORBIDDEN, ROOM_ACCESS_DENIED_MESSAGE);
        }
        Err(error) => return permission_error_response(error),
    };

    let items: Vec<RoomPermissionItem> = permissions
        .into_iter()
        .map(|(account_id, level)| {
            // Display information comes from the maitre-d account table; an
            // account that no longer exists still lists its permission.
            let account = state.db.get_account(account_id).ok().flatten();
            RoomPermissionItem {
                account_id,
                username: account
                    .as_ref()
                    .map(|account| account.handle_name.clone())
                    .unwrap_or_default(),
                icon_url: account
                    .as_ref()
                    .and_then(|account| account.icon_tin_id)
                    .map(|id| format!("/api/cellar/tin/{}", hex::encode(id))),
                role: RoomRole::from(level),
            }
        })
        .collect();

    (StatusCode::OK, Json(items)).into_response()
}

/// Handles `POST /api/room/{room_id}/permissions`.
///
/// Thin proxy over banquette's `SetMemberPermission`: the caller's account is
/// the caller identity, so banquette itself decides whether the change is
/// allowed (`admin` only). `maitre-d` writes nothing.
async fn set_room_permission_handler(
    State(state): State<AppState>,
    AxumPath(room_id): AxumPath<RoomId>,
    headers: HeaderMap,
    Query(query): Query<RoomSessionQuery>,
    Json(req): Json<SetRoomPermissionRequest>,
) -> Response {
    let caller =
        match resolve_authenticated_account(&state, &headers, query.account_id, query.session_id) {
            Ok(account_id) => account_id,
            Err(response) => return response.into_response(),
        };

    let (registered_room, bridge) = match resolve_room_bridge(&state, &room_id).await {
        Ok(values) => values,
        Err(response) => return response,
    };

    // The target account is a maitre-d entity, so it is validated here.
    match state.db.get_account(req.account_id) {
        Ok(Some(_)) => {}
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "Account not found"),
        Err(error) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }

    let permission = match permission_level(req.role) {
        Some(level) => level,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "role must not be denied for a member permission change",
            );
        }
    };

    match bridge
        .set_member_permission(caller, req.account_id, permission)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "room_id": registered_room.room_id,
                "account_id": req.account_id,
                "role": req.role
            })),
        )
            .into_response(),
        Err(error) => permission_error_response(error),
    }
}

/// Handles `DELETE /api/room/{room_id}/permissions/{account_id}`.
///
/// Removing a member means setting an explicit `Denied` entry, which overrides
/// the room's default permission in `RoomData.permissions`. banquette enforces
/// that only an administrator may do it.
async fn delete_room_permission_handler(
    State(state): State<AppState>,
    AxumPath((room_id, account_id)): AxumPath<(RoomId, u64)>,
    headers: HeaderMap,
    Query(query): Query<RoomSessionQuery>,
) -> Response {
    let caller =
        match resolve_authenticated_account(&state, &headers, query.account_id, query.session_id) {
            Ok(account_id) => account_id,
            Err(response) => return response.into_response(),
        };

    let (_room, bridge) = match resolve_room_bridge(&state, &room_id).await {
        Ok(values) => values,
        Err(response) => return response,
    };

    match bridge
        .set_member_permission(caller, account_id, RoomPermissionLevel::Denied)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(error) => permission_error_response(error),
    }
}

/// Maps a role that grants room access onto banquette's permission level.
///
/// `Denied` is not a member role; a permission change uses the delete route (or
/// an explicit `denied` body on the same route) instead.
fn permission_level(role: RoomRole) -> Option<RoomPermissionLevel> {
    match role {
        RoomRole::Admin => Some(RoomPermissionLevel::Admin),
        RoomRole::Participant => Some(RoomPermissionLevel::Participant),
        RoomRole::Viewer => Some(RoomPermissionLevel::Viewer),
        RoomRole::Denied => None,
    }
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
    let claimed_account_id = query
        .account_id
        .or_else(|| header_as_u64(&headers, "X-Alhalte-Account"));
    let session_id = query
        .session_id
        .or_else(|| header_as_u64(&headers, "X-Alhalte-Session"))
        .or_else(|| extract_session_id_from_cookie(&headers));

    // spec: alhalte-maitre-d/websocket-identity
    // - `account_id` alone is never proof of identity (401).
    // - `account_id` + a valid session is accepted when both agree.
    // - a session alone derives its account (no 401).
    // - neither is an anonymous connection, allowed only for a public room.
    let (auth_account, auth_session) = match (claimed_account_id, session_id) {
        (Some(_), None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "A valid session is required with account_id" })),
            )
                .into_response();
        }
        (None, None) => (0, 0),
        (claimed_account, Some(session_id)) => {
            let session = match state.db.get_session(session_id) {
                Ok(Some(session)) => session,
                Ok(None) => {
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
            };

            if let Some(claimed) = claimed_account
                && claimed != session.account_id
            {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Invalid session credentials" })),
                )
                    .into_response();
            }

            match state.db.is_session_valid(session.account_id, session_id) {
                Ok(true) => (session.account_id, session_id),
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
    };

    // A room must be registered in the room registry before it can be joined;
    // an unknown room is 404 and never starts a banquette process.
    let (_room, bridge) = match resolve_room_bridge(&state, &room_id).await {
        Ok(values) => values,
        Err(response) => return response,
    };

    // Authorization belongs to banquette: the room process is started first and
    // asked for the caller's effective permission. `0` is the anonymous account,
    // so an anonymous caller is judged by the room's default permission.
    let role = match resolve_room_role(&bridge, auth_account).await {
        Ok(role) => role,
        Err(response) => return response,
    };
    if let Err(response) = require_room_access(role, auth_account == 0) {
        return response;
    }

    let client_id = state.next_client_id();
    ws.on_upgrade(move |socket| {
        handle_websocket(
            socket,
            WsClientContext {
                bridge,
                client_id,
                account_id: auth_account,
                session_id: auth_session,
            },
        )
    })
}

struct WsClientContext {
    bridge: Arc<RoomBridge>,
    client_id: u64,
    account_id: u64,
    session_id: u64,
}

async fn handle_websocket(socket: WebSocket, ctx: WsClientContext) {
    let client_id = ctx.client_id;
    let account_id = ctx.account_id;
    let session_id = ctx.session_id;
    let bridge = ctx.bridge;

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
    //
    // Every frame is forwarded unchanged. Authorization (the viewer/full-access
    // split, scene grants, admin-only permission changes) is enforced by
    // banquette's room permission table; maitre-d keeps no gate of its own and
    // invents no `Error` responses (Issue #200).
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
                Ok(frame) => bridge_ref.send_client_frame(client_id, frame),
                Err(e) => {
                    tracing::warn!("Failed to deserialize ClientFrame from JSON: {e:?}");
                }
            },
            Message::Binary(bytes) => {
                let frame = postcard::from_bytes::<ClientFrame>(&bytes)
                    .or_else(|_| serde_json::from_slice::<ClientFrame>(&bytes));
                match frame {
                    Ok(f) => bridge_ref.send_client_frame(client_id, f),
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

    // Export never creates a room implicitly. An unknown room is 404 and never
    // starts a banquette process.
    let (_room, bridge) = match resolve_room_bridge(&state, &room_id).await {
        Ok(values) => values,
        Err(response) => return response,
    };

    // banquette is the authority: ask it for the caller's effective permission
    // (account `0` = the room default for an anonymous export) and reject
    // `denied` before any scene is read.
    let role = match resolve_room_role(&bridge, caller_account.unwrap_or(0)).await {
        Ok(role) => role,
        Err(response) => return response,
    };
    if let Err(response) = require_room_access(role, caller_account.is_none()) {
        return response;
    }

    // banquette assembles the ZIP (Issue #193): it is the authority on which
    // scenes the caller may export, so `type=logs` is honoured there by
    // emptying every scene state before the archive is written.
    let zip_bytes = match bridge
        .export_room(caller_account, export_type == "full")
        .await
    {
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
/// ownership is therefore never trusted: it is never read from the body, and a
/// body that still claims `owner` / `session_id` is rejected with `400`
/// (Issue #191).
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

/// Returns the first legacy ownership field found in a cellar body, if any.
///
/// Since Issue #191 the store handshake body is `{ tin_id, size, wrapped_key }`
/// only. `owner` / `session_id` used to be client-supplied and rewritten by the
/// proxy; both are now rejected with `400` so an old client fails loudly instead
/// of having its claim silently ignored.
fn claimed_owner_fields(payload: &serde_json::Value) -> Option<&'static str> {
    const LEGACY_FIELDS: [&str; 2] = ["owner", "session_id"];
    let map = payload.as_object()?;
    LEGACY_FIELDS
        .into_iter()
        .find(|field| map.contains_key(*field))
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
/// The upstream status and body are preserved verbatim: the store check's
/// `200 OK` + `{"result": ...}` body and ordinary error statuses (`400` /
/// `401` / `404` / `5xx`) reach the caller unchanged. Only the headers listed
/// in [`FORWARDED_RESPONSE_HEADERS`] are copied; transport failures become
/// `502` / `504`.
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
/// The caller's identity is injected as `X-Alhalte-Owner` / `X-Alhalte-Session-Id`;
/// the JSON body is forwarded unchanged and must not carry an ownership claim.
/// Since Issue #191 the body is `{ tin_id, size, wrapped_key }` only: a client
/// that still sends `owner` / `session_id` (the pre-#191 contract) is answered
/// with `400` instead of having its claim silently ignored. The upstream
/// `200 OK` + `{"result": ...}` answer is passed through verbatim.
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

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(payload @ serde_json::Value::Object(_)) => payload,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "request body must be a JSON object" })),
            )
                .into_response();
        }
    };

    if let Some(claim) = claimed_owner_fields(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "`{claim}` must not be sent in the body; ownership is derived from the session"
                ),
            })),
        )
            .into_response();
    }

    let mut upstream_headers = owner_headers(&caller);
    upstream_headers.push(("Content-Type", "application/json".to_string()));

    forward_cellar_request(
        proxy,
        reqwest::Method::POST,
        "/store",
        upstream_headers,
        Some(body),
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
