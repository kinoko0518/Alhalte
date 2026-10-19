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

/// Role assigned to an account in a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomRole {
    /// Game master / room administrator.
    Gm,
    /// Player.
    Pl,
    /// Spectator / read-only viewer.
    Viewer,
}

impl RoomRole {
    /// Returns the static string representation of this role.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gm => "gm",
            Self::Pl => "pl",
            Self::Viewer => "viewer",
        }
    }
}

impl std::str::FromStr for RoomRole {
    type Err = MaitreDError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "gm" => Ok(Self::Gm),
            "pl" => Ok(Self::Pl),
            "viewer" => Ok(Self::Viewer),
            other => Err(MaitreDError::InvalidRole(other.to_string())),
        }
    }
}

impl std::fmt::Display for RoomRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Room registered in the persistent room registry (`rooms` table).
///
/// The room ID is stored in its canonical 16-hex form. Rooms are created
/// explicitly through [`Database::create_room`]; a permission row alone does not
/// make a room joinable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    /// Canonical 16-hex room ID.
    pub room_id: String,
    /// Human-readable room name.
    pub name: String,
    /// Account that created the room (also inserted as the initial `gm`).
    pub creator_account: u64,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at: i64,
}

/// Room permission entry returned by query and listing endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPermissionItem {
    /// 64-bit account ID.
    pub account_id: u64,
    /// User name / handle name.
    pub username: String,
    /// Avatar icon URL if available.
    pub icon_url: Option<String>,
    /// Assigned role in the room.
    pub role: RoomRole,
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

            CREATE TABLE IF NOT EXISTS rooms (
                room_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                creator_account INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS room_permissions (
                room_id TEXT NOT NULL,
                account_id INTEGER NOT NULL,
                role TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (room_id, account_id)
            );
            CREATE INDEX IF NOT EXISTS idx_room_permissions_room ON room_permissions(room_id);
            CREATE INDEX IF NOT EXISTS idx_room_permissions_account ON room_permissions(account_id);
            ",
        )?;

        // Bring pre-registry data forward: permission rows written before the
        // canonical room ID contract used decimal IDs, and rooms created before
        // this table existed have permission rows but no registry row.
        Self::normalize_legacy_room_permissions(&conn)?;
        Self::backfill_room_registry(&conn)?;

        Ok(())
    }

    /// Placeholder name used when a room is recovered from its permission rows.
    const RECOVERED_ROOM_NAME: &'static str = "Unnamed room";

    /// Rewrites legacy decimal `room_permissions.room_id` values to canonical
    /// 16-hex form (spec: `room_permissions` stores canonical room IDs).
    ///
    /// A 16-character all-digit value is left untouched because the room ID
    /// contract interprets it as hex; only shorter/longer decimal strings are
    /// rewritten. When a canonical row already exists for the same account, the
    /// canonical row wins and the legacy duplicate is dropped.
    fn normalize_legacy_room_permissions(conn: &Connection) -> Result<()> {
        let legacy: Vec<(String, i64, String, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT room_id, account_id, role, created_at
                 FROM room_permissions
                 WHERE length(room_id) <> 16
                   AND room_id <> ''
                   AND room_id NOT GLOB '*[^0-9]*'",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        for (legacy_id, account_id, role, created_at) in legacy {
            let Ok(value) = legacy_id.parse::<u64>() else {
                continue;
            };
            let canonical = format!("{value:016x}");
            conn.execute(
                "INSERT OR IGNORE INTO room_permissions (room_id, account_id, role, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![canonical, account_id, role, created_at],
            )?;
            conn.execute(
                "DELETE FROM room_permissions WHERE room_id = ?1 AND account_id = ?2",
                params![legacy_id, account_id],
            )?;
        }

        Ok(())
    }

    /// Backfills `rooms` for permission rows that predate the room registry.
    ///
    /// The creator is the earliest `gm` (or, failing that, the earliest account
    /// with any role) so that the recovered room has a stable bridge admin. The
    /// original room name was client-side only, so a placeholder is used.
    fn backfill_room_registry(conn: &Connection) -> Result<()> {
        let missing: Vec<(String, i64, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT p.room_id,
                        COALESCE(
                            (SELECT g.account_id FROM room_permissions g
                             WHERE g.room_id = p.room_id AND g.role = 'gm'
                             ORDER BY g.created_at ASC, g.account_id ASC LIMIT 1),
                            (SELECT a.account_id FROM room_permissions a
                             WHERE a.room_id = p.room_id
                             ORDER BY a.created_at ASC, a.account_id ASC LIMIT 1)
                        ),
                        COALESCE(
                            (SELECT MIN(c.created_at) FROM room_permissions c
                             WHERE c.room_id = p.room_id),
                            0
                        )
                 FROM (SELECT DISTINCT room_id FROM room_permissions) p
                 WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = p.room_id)",
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        for (room_id, creator_account, created_at) in missing {
            conn.execute(
                "INSERT OR IGNORE INTO rooms (room_id, name, creator_account, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    room_id,
                    Self::RECOVERED_ROOM_NAME,
                    creator_account,
                    created_at
                ],
            )?;
        }

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

    /// Registers a room and inserts `creator_account` as its initial `gm`.
    ///
    /// The caller is responsible for supplying a canonical 16-hex `room_id` and
    /// a validated `name`; the HTTP layer performs both checks.
    ///
    /// # Errors
    ///
    /// Returns [`MaitreDError::RoomAlreadyExists`] when the room ID is already
    /// registered. The room and its initial permission are inserted in one
    /// transaction, so a failure leaves neither behind.
    pub fn create_room(
        &self,
        room_id: &str,
        name: &str,
        creator_account: u64,
        now: i64,
    ) -> Result<Room> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM rooms WHERE room_id = ?1",
                params![room_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(MaitreDError::RoomAlreadyExists(room_id.to_string()));
        }

        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO rooms (room_id, name, creator_account, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![room_id, name, creator_account as i64, now],
        )?;
        tx.execute(
            "INSERT INTO room_permissions (room_id, account_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (room_id, account_id) DO UPDATE SET role = excluded.role",
            params![room_id, creator_account as i64, RoomRole::Gm.as_str(), now],
        )?;
        tx.commit()?;

        Ok(Room {
            room_id: room_id.to_string(),
            name: name.to_string(),
            creator_account,
            created_at: now,
        })
    }

    /// Looks up a registered room by its canonical 16-hex ID.
    pub fn get_room(&self, room_id: &str) -> Result<Option<Room>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT room_id, name, creator_account, created_at FROM rooms WHERE room_id = ?1",
        )?;

        let room = stmt
            .query_row(params![room_id], |row| {
                let creator_account: i64 = row.get(2)?;
                Ok(Room {
                    room_id: row.get(0)?,
                    name: row.get(1)?,
                    creator_account: creator_account as u64,
                    created_at: row.get(3)?,
                })
            })
            .optional()?;

        Ok(room)
    }

    /// Sets or updates the permission role for an account in a room.
    pub fn set_room_permission(
        &self,
        room_id: &str,
        account_id: u64,
        role: RoomRole,
        now: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        conn.execute(
            "INSERT INTO room_permissions (room_id, account_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (room_id, account_id) DO UPDATE SET role = excluded.role",
            params![room_id, account_id as i64, role.as_str(), now],
        )?;

        Ok(())
    }

    /// Deletes a permission entry for an account in a room.
    pub fn delete_room_permission(&self, room_id: &str, account_id: u64) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let affected = conn.execute(
            "DELETE FROM room_permissions WHERE room_id = ?1 AND account_id = ?2",
            params![room_id, account_id as i64],
        )?;

        Ok(affected > 0)
    }

    /// Retrieves all permissions set for a room, along with user handle name and icon URL.
    pub fn get_room_permissions(&self, room_id: &str) -> Result<Vec<RoomPermissionItem>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT p.account_id, COALESCE(a.handle_name, ''), a.icon_tin_id, p.role
             FROM room_permissions p
             LEFT JOIN accounts a ON p.account_id = a.account_id
             WHERE p.room_id = ?1
             ORDER BY p.created_at ASC, p.account_id ASC",
        )?;

        let mut items = Vec::new();
        let mut rows = stmt.query(params![room_id])?;
        while let Some(row) = rows.next()? {
            let aid: i64 = row.get(0)?;
            let username: String = row.get(1)?;
            let icon_blob: Option<Vec<u8>> = row.get(2)?;
            let role_str: String = row.get(3)?;

            let icon_tin_id = icon_blob.and_then(|b| {
                if b.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&b);
                    Some(arr)
                } else {
                    None
                }
            });

            let icon_url = icon_tin_id.map(|id| format!("/api/cellar/tin/{}", hex::encode(id)));
            let role = role_str.parse::<RoomRole>()?;

            items.push(RoomPermissionItem {
                account_id: aid as u64,
                username,
                icon_url,
                role,
            });
        }

        Ok(items)
    }

    /// Checks whether any room permissions are configured for a room.
    pub fn has_room_permissions(&self, room_id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare("SELECT 1 FROM room_permissions WHERE room_id = ?1 LIMIT 1")?;
        let exists = stmt.query_row(params![room_id], |_| Ok(true)).optional()?;
        Ok(exists.unwrap_or(false))
    }

    /// Retrieves the role of a specific account in a room.
    pub fn get_room_role(&self, room_id: &str, account_id: u64) -> Result<Option<RoomRole>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT role FROM room_permissions WHERE room_id = ?1 AND account_id = ?2 LIMIT 1",
        )?;

        let role_str: Option<String> = stmt
            .query_row(params![room_id, account_id as i64], |row| row.get(0))
            .optional()?;

        match role_str {
            Some(s) => Ok(Some(s.parse::<RoomRole>()?)),
            None => Ok(None),
        }
    }

    /// Retrieves the first GM account ID configured for a room.
    pub fn get_room_gm(&self, room_id: &str) -> Result<Option<u64>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(
            "SELECT account_id FROM room_permissions WHERE room_id = ?1 AND role = 'gm' ORDER BY created_at ASC LIMIT 1",
        )?;

        let gm_id: Option<i64> = stmt
            .query_row(params![room_id], |row| row.get(0))
            .optional()?;

        Ok(gm_id.map(|id| id as u64))
    }

    /// Searches accounts by username or account ID.
    pub fn search_accounts(&self, query: &str) -> Result<Vec<Account>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let parsed_id = query.parse::<i64>().ok();
        let like_pattern = format!("%{query}%");

        let mut stmt;
        let mut rows = match parsed_id {
            Some(id) => {
                stmt = conn.prepare(
                    "SELECT account_id, encrypted_sub, handle_name, icon_tin_id, created_at
                     FROM accounts
                     WHERE account_id = ?1 OR handle_name LIKE ?2
                     ORDER BY (account_id = ?1) DESC, account_id ASC
                     LIMIT 50",
                )?;
                stmt.query(params![id, like_pattern])?
            }
            None => {
                stmt = conn.prepare(
                    "SELECT account_id, encrypted_sub, handle_name, icon_tin_id, created_at
                     FROM accounts
                     WHERE handle_name LIKE ?1
                     ORDER BY account_id ASC
                     LIMIT 50",
                )?;
                stmt.query(params![like_pattern])?
            }
        };

        let mut accounts = Vec::new();
        while let Some(row) = rows.next()? {
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

            accounts.push(Account {
                account_id: account_id as u64,
                encrypted_sub,
                handle_name,
                icon_tin_id,
                created_at,
            });
        }

        Ok(accounts)
    }
}
