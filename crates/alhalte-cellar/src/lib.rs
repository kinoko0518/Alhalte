//! # alhalte-cellar
//!
//! Deduplicated content-addressable storage service and garbage collector for the Alhalte ecosystem.
//!
//! ## Overview
//!
//! `alhalte-cellar` stores binary `tin` files as physical entities on disk and indexes references
//! as rows in an SQLite database: `(tin_id: [u8; 32], path: String, owner: u64, wrapped_key: Option<Vec<u8>>)`.
//!
//! When receiving a store request:
//! - Validates the owner's session rights via [`SessionVerifier`].
//! - If the tin is not yet stored: returns status **210** (Upload Required).
//! - If the tin is already stored by another owner: links the existing entity, inserts a new DB reference row, and returns **211** (Deduplicated).
//! - If the tin is already owned by the same owner: returns **212** (Already Owned).
//! - In **anti-oracle** mode (`--anti-oracle true`): always returns **210** and discards the uploaded payload into `/dev/null` if the entity is already present.
//!
//! ## Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use alhalte_cellar::{CellarConfig, CellarService, InMemorySessionVerifier, StoreRequest, StoreStatus};
//! use tempfile::tempdir;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dir = tempdir()?;
//! let verifier = Arc::new(InMemorySessionVerifier::new());
//! verifier.allow(42, 100);
//!
//! let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false)?;
//! let req = StoreRequest {
//!     owner: 42,
//!     session_id: 100,
//!     tin_id: [1u8; 32],
//!     size: 10,
//!     wrapped_key: None,
//! };
//!
//! let status = service.store_check(&req).await?;
//! assert_eq!(status, StoreStatus::UploadRequired);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod db;
pub mod error;
pub mod gc;
pub mod handlers;
pub mod model;
pub mod service;
pub mod storage;
pub mod verifier;

pub use cli::{CliArgs, parse_cli_args};

pub use db::Database;
pub use error::{CellarError, Result};
pub use gc::run_garbage_collection;
pub use handlers::create_router;
pub use model::{
    GcStats, StoreRequest, StoreResponse, StoreResult, StoreStatus, TinReference,
    UnregisterRequest, format_tin_id, parse_tin_id,
};
pub use service::{CellarConfig, CellarService};
pub use storage::StorageManager;
pub use tin::TinId;
pub use verifier::{InMemorySessionVerifier, MaitreDTarget, MaitreDVerifier, SessionVerifier};
