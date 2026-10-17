//! SQLite database operations for account and session management.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::crypto::DEBUG_SUB_CIPHERTEXT;
use crate::error::{MaitreDError, Result};

/// Account entity stored in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// 64-bit account ID (`alhalte/maitre-d/account-id`).
    pub account_id: u64,
    /// Encrypted Google `sub` (or `"0"` for debug account).
    pub encrypted_sub: String,
    /// Handle name / nickname.
    pub handle_name: String,
    /// Optional 32-byte Tin ID pointing to the user's avatar icon.
    pub icon_tin_id: Option<[u8; 32]>,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at: i64,
}

/// Active session entity stored in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// 64-bit session ID (`alhalte/maitre-d/session-id`).
    pub session_id: u64,
    /// Owning account ID.
    pub account_id: u64,
    /// Timestamp of most recent heartbeat (seconds since Unix epoch).
    pub last_heartbeat: i64,
    /// Timestamp when session was created (seconds since Unix epoch).
    pub created_at: i64,
}

/// Thread-safe SQLite database manager for accounts and sessions.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens or creates an SQLite database at the specified filesystem path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_tables()?;
        Ok(db)
    }

    /// Creates an in-memory SQLite database (ideal for tests).
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_tables()?;
        Ok(db)
    }

    /// Initializes tables and indexes.
    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS accounts (
                account_id INTEGER PRIMARY KEY,
                encrypted_sub TEXT NOT NULL,
                handle_name TEXT NOT NULL,
                icon_tin_id BLOB,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_accounts_sub ON accounts(encrypted_sub);

            CREATE TABLE IF NOT EXISTS sessions (
                session_id INTEGER PRIMARY KEY,
                account_id INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
                last_heartbeat INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_account ON sessions(account_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_heartbeat ON sessions(last_heartbeat);
            ",
        )?;

        Ok(())
    }

    /// Looks up an account by `encrypted_sub`. If it does not exist, inserts a new account.
    pub fn get_or_create_account(
        &self,
        encrypted_sub: &str,
        default_handle_name: &str,
        icon_tin_id: Option<[u8; 32]>,
        now: i64,
    ) -> Result<Account> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT account_id, encrypted_sub, handle_name, icon_tin_id, created_at FROM accounts WHERE encrypted_sub = ?1 LIMIT 1",
        )?;

        let existing = stmt
            .query_row(params![encrypted_sub], |row| {
                let account_id: i64 = row.get(0)?;
                let encrypted_sub: String = row.get(1)?;
                let handle_name: String = row.get(2)?;
                let icon_blob: Option<Vec<u8>> = row.get(3)?;
                let created_at: i64 = row.get(4)?;

                let icon_tin_id = icon_blob.and_then(|b| {
                    if b.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&b);
                        Some(arr)
                    } else {
                        None
                    }
                });

                Ok(Account {
                    account_id: account_id as u64,
                    encrypted_sub,
                    handle_name,
                    icon_tin_id,
                    created_at,
                })
            })
            .optional()?;

        if let Some(account) = existing {
            return Ok(account);
        }

        // Generate next monotonic account ID
        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(account_id), 0) + 1 FROM accounts",
            [],
            |r| r.get(0),
        )?;

        let icon_bytes = icon_tin_id.map(|id| id.to_vec());

        conn.execute(
            "INSERT INTO accounts (account_id, encrypted_sub, handle_name, icon_tin_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                next_id,
                encrypted_sub,
                default_handle_name,
                icon_bytes,
                now
            ],
        )?;

        Ok(Account {
            account_id: next_id as u64,
            encrypted_sub: encrypted_sub.to_string(),
            handle_name: default_handle_name.to_string(),
            icon_tin_id,
            created_at: now,
        })
    }

    /// Retrieves an account by its 64-bit `account_id`.
    pub fn get_account(&self, account_id: u64) -> Result<Option<Account>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT account_id, encrypted_sub, handle_name, icon_tin_id, created_at FROM accounts WHERE account_id = ?1",
        )?;

        let account = stmt
            .query_row(params![account_id as i64], |row| {
                let account_id: i64 = row.get(0)?;
                let encrypted_sub: String = row.get(1)?;
                let handle_name: String = row.get(2)?;
                let icon_blob: Option<Vec<u8>> = row.get(3)?;
                let created_at: i64 = row.get(4)?;

                let icon_tin_id = icon_blob.and_then(|b| {
                    if b.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&b);
                        Some(arr)
                    } else {
                        None
                    }
                });

                Ok(Account {
                    account_id: account_id as u64,
                    encrypted_sub,
                    handle_name,
                    icon_tin_id,
                    created_at,
                })
            })
            .optional()?;

        Ok(account)
    }

    /// Retrieves an account by `encrypted_sub`.
    pub fn get_account_by_sub(&self, encrypted_sub: &str) -> Result<Option<Account>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT account_id, encrypted_sub, handle_name, icon_tin_id, created_at FROM accounts WHERE encrypted_sub = ?1 LIMIT 1",
        )?;

        let account = stmt
            .query_row(params![encrypted_sub], |row| {
                let account_id: i64 = row.get(0)?;
                let encrypted_sub: String = row.get(1)?;
                let handle_name: String = row.get(2)?;
                let icon_blob: Option<Vec<u8>> = row.get(3)?;
                let created_at: i64 = row.get(4)?;

                let icon_tin_id = icon_blob.and_then(|b| {
                    if b.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&b);
                        Some(arr)
                    } else {
                        None
                    }
                });

                Ok(Account {
                    account_id: account_id as u64,
                    encrypted_sub,
                    handle_name,
                    icon_tin_id,
                    created_at,
                })
            })
            .optional()?;

        Ok(account)
    }

    /// Updates the handle name and/or avatar icon for an account.
    pub fn update_account_profile(
        &self,
        account_id: u64,
        handle_name: Option<&str>,
        icon_tin_id: Option<Option<[u8; 32]>>,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        if let Some(name) = handle_name {
            let affected = conn.execute(
                "UPDATE accounts SET handle_name = ?1 WHERE account_id = ?2",
                params![name, account_id as i64],
            )?;
            if affected == 0 {
                return Err(MaitreDError::AccountNotFound(account_id));
            }
        }

        if let Some(icon_opt) = icon_tin_id {
            let icon_blob = icon_opt.map(|id| id.to_vec());
            let affected = conn.execute(
                "UPDATE accounts SET icon_tin_id = ?1 WHERE account_id = ?2",
                params![icon_blob, account_id as i64],
            )?;
            if affected == 0 {
                return Err(MaitreDError::AccountNotFound(account_id));
            }
        }

        Ok(())
    }

    /// Creates a new debug account with `encrypted_sub = "0"`.
    ///
    /// Available in debug builds (`#[cfg(debug_assertions)]`).
    #[cfg(debug_assertions)]
    pub fn create_debug_account(
        &self,
        handle_name: &str,
        icon_tin_id: Option<[u8; 32]>,
        now: i64,
    ) -> Result<Account> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(account_id), 0) + 1 FROM accounts",
            [],
            |r| r.get(0),
        )?;

        let icon_bytes = icon_tin_id.map(|id| id.to_vec());

        conn.execute(
            "INSERT INTO accounts (account_id, encrypted_sub, handle_name, icon_tin_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                next_id,
                DEBUG_SUB_CIPHERTEXT,
                handle_name,
                icon_bytes,
                now
            ],
        )?;

        Ok(Account {
            account_id: next_id as u64,
            encrypted_sub: DEBUG_SUB_CIPHERTEXT.to_string(),
            handle_name: handle_name.to_string(),
            icon_tin_id,
            created_at: now,
        })
    }

    /// Issues a new 64-bit session ID for the given account.
    pub fn create_session(&self, account_id: u64, now: i64) -> Result<Session> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        // Generate non-zero random 64-bit session ID
        let mut session_id = rand::random::<u64>();
        while session_id == 0 {
            session_id = rand::random::<u64>();
        }

        conn.execute(
            "INSERT INTO sessions (session_id, account_id, last_heartbeat, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![session_id as i64, account_id as i64, now, now],
        )?;

        Ok(Session {
            session_id,
            account_id,
            last_heartbeat: now,
            created_at: now,
        })
    }

    /// Records a heartbeat for an active session, updating `last_heartbeat`.
    ///
    /// Returns `true` if session was found and updated, `false` otherwise.
    pub fn record_heartbeat(&self, session_id: u64, now: i64) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let affected = conn.execute(
            "UPDATE sessions SET last_heartbeat = ?1 WHERE session_id = ?2",
            params![now, session_id as i64],
        )?;

        Ok(affected > 0)
    }

    /// Checks whether `(account_id, session_id)` represents a currently active session.
    pub fn is_session_valid(&self, account_id: u64, session_id: u64) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt =
            conn.prepare("SELECT 1 FROM sessions WHERE session_id = ?1 AND account_id = ?2")?;

        let exists = stmt
            .query_row(params![session_id as i64, account_id as i64], |_| Ok(true))
            .optional()?;

        Ok(exists.unwrap_or(false))
    }

    /// Retrieves session information by `session_id`.
    pub fn get_session(&self, session_id: u64) -> Result<Option<Session>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT session_id, account_id, last_heartbeat, created_at FROM sessions WHERE session_id = ?1",
        )?;

        let session = stmt
            .query_row(params![session_id as i64], |row| {
                let sid: i64 = row.get(0)?;
                let aid: i64 = row.get(1)?;
                let hb: i64 = row.get(2)?;
                let ca: i64 = row.get(3)?;
                Ok(Session {
                    session_id: sid as u64,
                    account_id: aid as u64,
                    last_heartbeat: hb,
                    created_at: ca,
                })
            })
            .optional()?;

        Ok(session)
    }

    /// Deletes a session explicitly (e.g., logout).
    pub fn delete_session(&self, session_id: u64) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let affected = conn.execute(
            "DELETE FROM sessions WHERE session_id = ?1",
            params![session_id as i64],
        )?;

        Ok(affected > 0)
    }

    /// Deletes all sessions whose `last_heartbeat` is strictly before `cutoff_timestamp`.
    ///
    /// Returns the number of expired sessions deleted.
    pub fn cleanup_sessions_before(&self, cutoff_timestamp: i64) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let affected = conn.execute(
            "DELETE FROM sessions WHERE last_heartbeat < ?1",
            params![cutoff_timestamp],
        )?;

        Ok(affected)
    }
}
