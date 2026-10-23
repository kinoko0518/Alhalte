//! # alhalte-maitre-d
//!
//! Authentication, SQLite account and session management, and WebSocket room demultiplexer
//! for the Alhalte ecosystem.
//!
//! ## Overview
//!
//! `alhalte-maitre-d` acts as the primary communication gateway and authentication coordinator:
//! - **Google OAuth 2.0 & SQLite Account Table**: Delegates authentication to Google OAuth, inheriting
//!   only the `sub` identifier. The raw `sub` is never stored: accounts keep a 128-bit, instance-salt
//!   keyed BLAKE3 hash (`sub_hash`) plus a 64-bit account ID (`alhalte/maitre-d/account-id`), handle
//!   name, and optional avatar Tin ID.
//! - **Session Expiration at 4:00 AM**: Issues 64-bit session IDs (`alhalte/maitre-d/session-id`) with heartbeat
//!   tracking. Every day at 4:00 AM local time, all sessions without a heartbeat since the previous 4:00 AM
//!   are purged.
//! - **Session Verification & Local Login**: Exposes verification endpoints compatible with
//!   `alhalte-cellar` and, with `--local-login`, a password login backed by a separate SQLite file
//!   ([`CliArgs::active_db_path`]). Local accounts never mix with Google accounts. Room state is still
//!   persisted to cellar normally.
//! - **WebSocket Demultiplexer & Banquette Spawner**: Inspects incoming WebSocket headers (`X-Alhalte-Room`),
//!   query parameters (`room`), and URL paths (`/room/{room_id}`) to demultiplex messages to `alhalte-banquette`
//!   daemons via a local transport (UNIX socket on Unix, named pipe on Windows). Spawning and the
//!   transport connection are awaited before a client is registered, so a failed room restore is reported
//!   to the caller instead of silently starting empty.
//! - **Room state head**: owned by cellar, not by maitre-d. Banquette keeps the last persisted `RoomData`
//!   under the alias `room/<room_id>/head` and restores itself from it on startup, so `maitre-d` passes no
//!   head to the spawner and stores none (Issue #192).
//!
//! ## Example
//!
//! ```
//! use alhalte_maitre_d::{Database, account_id_to_tin, account_id_from_tin, hash_sub};
//!
//! let db = Database::new_in_memory().unwrap();
//! let mut sub = "google-sub-123".to_string();
//! let account = db
//!     .get_or_create_account(&hash_sub("instance-salt", &mut sub), "Alice", None, 1000)
//!     .unwrap();
//! assert_eq!(account.account_id, 1);
//! // `hash_sub` zeroizes the raw identifier.
//! assert!(sub.is_empty());
//!
//! let tin = account_id_to_tin(account.account_id);
//! assert_eq!(account_id_from_tin(&tin).unwrap(), 1);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod auth;
pub mod cli;
pub mod crypto;
pub mod db;
pub mod error;
pub mod gateway;
pub mod labels;
pub mod oauth;
pub mod password;
pub mod room_id;
pub mod secret_file;
pub mod server;
pub mod session;
pub mod spawner;

pub use auth::{
    AuthIdentity, AuthKind, AuthProvider, DEFAULT_HANDLE_NAME, GoogleAuthProvider,
    LOCAL_SUBJECT_PREFIX, LocalAuthProvider, MAX_ACCOUNT_NAME_LEN, parse_local_account_name,
    percent_encode,
};
pub use cli::{CliArgs, LOCAL_DB_FILE_NAME, parse_cli_args, parse_cli_args_from};
pub use crypto::{SUB_HASH_BYTES, SUB_HASH_HEX_LEN, SubHash, hash_sub};
pub use db::{Account, Database, LocalAccount, OAUTH_STATE_TTL_SECS, Room, Session};
pub use error::{MaitreDError, Result};
pub use gateway::{RoomBridge, RoomBridgeManager};
pub use labels::{
    IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID, IDENT_ALHALTE_MAITRE_D_SESSION_ID,
    LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, LABEL_ALHALTE_MAITRE_D_SESSION_ID, account_id_from_tin,
    account_id_to_tin, session_id_from_tin, session_id_to_tin,
};
pub use oauth::{GoogleOAuthProvider, GoogleUserInfo, MockOAuthProvider, OAuthProvider};
pub use password::{
    MAX_PASSWORD_LEN, MIN_PASSWORD_LEN, hash_password, validate_password, verify_password,
};
pub use room_id::{RoomId, RoomIdParseError, parse_room_id};
pub use server::{
    AccountResponse, AccountSearchItem, AppState, AuthResponse, CreateRoomRequest,
    DEFAULT_ALLOWED_ORIGINS, DEFAULT_PUBLIC_URL, LocalLoginRequest, MAX_ROOM_NAME_LEN,
    OAuthLoginQuery, RoomInfoResponse, RoomPermissionItem, RoomRole, RoomSessionQuery,
    SESSION_COOKIE_MAX_AGE_SECS, SearchAccountsQuery, SessionResponse, SetRoomPermissionRequest,
    VerifySessionQuery, VerifySessionResponse, create_router, extract_session_id_from_cookie,
    is_allowed_return_to, is_allowed_return_to_with_lan, parse_session_id_from_cookie_str,
    session_cookie_header,
};
pub use session::{
    cleanup_sessions_at, cleanup_sessions_with_cutoff, duration_until_next_4am, previous_4am_local,
    spawn_session_cleanup_task,
};
#[cfg(unix)]
pub use spawner::InProcessBanquetteSpawner;
pub use spawner::{
    BanquetteEndpoint, BanquetteSpawner, ProcessBanquetteSpawner, resolve_banquette_bin,
};
