//! # alhalte-maitre-d
//!
//! Authentication, SQLite account and session management, and WebSocket room demultiplexer
//! for the Alhalte ecosystem.
//!
//! ## Overview
//!
//! `alhalte-maitre-d` acts as the primary communication gateway and authentication coordinator:
//! - **Google OAuth 2.0 & SQLite Account Table**: Delegates authentication to Google OAuth, inheriting
//!   only the `sub` identifier. Accounts are stored in SQLite with a 64-bit account ID (`alhalte/maitre-d/account-id`),
//!   an instance-salt encrypted `sub`, handle name, and optional avatar Tin ID.
//! - **Session Expiration at 4:00 AM**: Issues 64-bit session IDs (`alhalte/maitre-d/session-id`) with heartbeat
//!   tracking. Every day at 4:00 AM local time, all sessions without a heartbeat since the previous 4:00 AM
//!   are purged.
//! - **Session Verification & Debug Accounts**: Exposes verification endpoints compatible with `alhalte-cellar`
//!   and supports creating debug accounts with `sub = "0"` under debug builds.
//! - **WebSocket Demultiplexer & Banquette Spawner**: Inspects incoming WebSocket headers (`X-Alhalte-Room`),
//!   query parameters (`room`), and URL paths (`/room/{room_id}`) to demultiplex messages to `alhalte-banquette`
//!   daemons via UNIX domain sockets, buffering initial traffic while launching room processes on demand.
//!
//! ## Example
//!
//! ```
//! use alhalte_maitre_d::{Database, account_id_to_tin, account_id_from_tin};
//!
//! let db = Database::new_in_memory().unwrap();
//! let account = db.get_or_create_account("encrypted-sub-123", "Alice", None, 1000).unwrap();
//! assert_eq!(account.account_id, 1);
//!
//! let tin = account_id_to_tin(account.account_id);
//! assert_eq!(account_id_from_tin(&tin).unwrap(), 1);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod crypto;
pub mod db;
pub mod error;
pub mod gateway;
pub mod labels;
pub mod oauth;
pub mod room_id;
pub mod server;
pub mod session;
pub mod spawner;

pub use cli::{CliArgs, parse_cli_args, parse_cli_args_from};
pub use crypto::{DEBUG_SUB_CIPHERTEXT, decrypt_sub, encrypt_sub};
pub use db::{Account, Database, RoomPermissionItem, RoomRole, Session};
pub use error::{MaitreDError, Result};
pub use gateway::{RoomBridge, RoomBridgeManager};
pub use labels::{
    IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID, IDENT_ALHALTE_MAITRE_D_SESSION_ID,
    LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, LABEL_ALHALTE_MAITRE_D_SESSION_ID, account_id_from_tin,
    account_id_to_tin, session_id_from_tin, session_id_to_tin,
};
pub use oauth::{GoogleOAuthProvider, GoogleUserInfo, MockOAuthProvider, OAuthProvider};
pub use room_id::{RoomId, RoomIdParseError, parse_room_id};
pub use server::{
    AccountSearchItem, AppState, AuthResponse, SearchAccountsQuery, SetRoomPermissionRequest,
    VerifySessionQuery, VerifySessionResponse, create_router, extract_session_id_from_cookie,
    is_mutate_frame, parse_session_id_from_cookie_str,
};
pub use session::{
    cleanup_sessions_at, cleanup_sessions_with_cutoff, duration_until_next_4am, previous_4am_local,
    spawn_session_cleanup_task,
};
pub use spawner::{
    BanquetteSpawner, InProcessBanquetteSpawner, ProcessBanquetteSpawner, resolve_banquette_bin,
};
