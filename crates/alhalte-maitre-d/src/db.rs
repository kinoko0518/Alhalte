//! SQLite database operations for account and session management.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::crypto::SubHash;
use crate::error::{MaitreDError, Result};

/// Lifetime of a pending Google OAuth state, in seconds.
///
/// A state older than this is rejected by `GET /auth/google/callback` even when
/// it was never redeemed.
pub const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;

/// Account entity stored in SQLite.
///
/// Deliberately not `Serialize`: the stored [`SubHash`] is a pseudonymous
/// account correlator that must never be handed to a client. The HTTP layer
/// answers with [`crate::server::AccountResponse`] instead, which omits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// 64-bit account ID (`alhalte/maitre-d/account-id`).
    pub account_id: u64,
    /// 128-bit salted hash of the Google `sub` (never the raw identifier).
    pub sub_hash: SubHash,
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

/// Room registered in the persistent room registry (`rooms` table).
///
/// The registry holds identity and ownership only. Authorization (`Denied` /
/// `Viewer` / `Participant` / `Admin`) lives in the room's `RoomData` inside
/// `alhalte-banquette`; `maitre-d` never stores it (Issue #200).
///
/// The room ID is stored in its canonical 16-hex form. Rooms are created
/// explicitly through [`Database::create_room`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    /// Canonical 16-hex room ID.
    pub room_id: String,
    /// Human-readable room name.
    pub name: String,
    /// Account that created the room (also the initial room `Admin` at spawn).
    pub creator_account: u64,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at: i64,
}

/// Column list of `accounts` in the order [`account_from_row`] expects.
const ACCOUNT_COLUMNS: &str = "account_id, sub_hash, handle_name, icon_tin_id, created_at";

/// Reads the `sub_hash` column at `index` and parses its stored hex form.
///
/// A row that does not hold a 32-hex hash is corrupt (or a pre-#190 row that
/// slipped past [`Database::reject_legacy_account_schema`]).
fn read_sub_hash(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<SubHash> {
    let stored: String = row.get(index)?;
    SubHash::from_hex(&stored).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

/// Maps an `accounts` row selected as [`ACCOUNT_COLUMNS`] to an [`Account`].
fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let account_id: i64 = row.get(0)?;
    let icon_blob: Option<Vec<u8>> = row.get(3)?;
    let icon_tin_id = icon_blob.and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());

    Ok(Account {
        account_id: account_id as u64,
        sub_hash: read_sub_hash(row, 1)?,
        handle_name: row.get(2)?,
        icon_tin_id,
        created_at: row.get(4)?,
    })
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

        // `accounts` used to store a reversible ciphertext of the Google `sub`.
        // Issue #190 replaced it with a one-way salted hash, and no account
        // existed before it, so an old database is refused instead of migrated.
        Self::reject_legacy_account_schema(&conn)?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS accounts (
                account_id INTEGER PRIMARY KEY,
                sub_hash TEXT NOT NULL UNIQUE,
                handle_name TEXT NOT NULL,
                icon_tin_id BLOB,
                created_at INTEGER NOT NULL
            );

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

            CREATE TABLE IF NOT EXISTS room_heads (
                room_id TEXT PRIMARY KEY,
                tin_id BLOB NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS oauth_states (
                state TEXT PRIMARY KEY,
                return_to TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_oauth_states_created ON oauth_states(created_at);
            ",
        )?;

        // `rooms` and `room_permissions` used to hold the room authorization
        // model. It now lives in banquette's `RoomData`, so an old database must
        // be refused (or, when the legacy artifacts are empty, cleaned up)
        // before any of the remaining queries run.
        Self::retire_legacy_permission_schema(&conn)?;

        Ok(())
    }

    /// Refuses a pre-#190 `accounts` table.
    ///
    /// Issue #190 replaced `accounts.encrypted_sub` (reversible AES-GCM-SIV
    /// ciphertext) with `accounts.sub_hash` (a one-way 128-bit salted hash).
    /// Accounts were never created before that change, so there is no
    /// migration: a database whose `accounts` table still has the legacy
    /// column, or lacks `sub_hash`, is refused with an explicit error telling
    /// the operator to reset the database.
    ///
    /// spec: alhalte-maitre-d/accounts 0001 no migration from the encrypted_sub schema
    fn reject_legacy_account_schema(conn: &Connection) -> Result<()> {
        let has_accounts_table: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master
                           WHERE type = 'table' AND name = 'accounts')",
            [],
            |row| row.get(0),
        )?;
        if !has_accounts_table {
            return Ok(());
        }

        let has_encrypted_sub: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('accounts') WHERE name = 'encrypted_sub')",
            [],
            |row| row.get(0),
        )?;
        let has_sub_hash: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('accounts') WHERE name = 'sub_hash')",
            [],
            |row| row.get(0),
        )?;

        if has_encrypted_sub || !has_sub_hash {
            return Err(MaitreDError::LegacySubSchema);
        }

        Ok(())
    }

    /// Retires the pre-#200 room authorization schema.
    ///
    /// Issue #200 moved the source of truth for room permissions to
    /// `alhalte-banquette`'s `RoomData`. There is no migration: a database that
    /// still holds `room_permissions` rows or `rooms.done = 1` rows is refused
    /// with an explicit error telling the operator to reset the database.
    ///
    /// When the legacy artifacts are present but empty they are dropped, so a
    /// database created by a previous build keeps working.
    ///
    /// spec: alhalte-maitre-d/permissions 0001 maitre-d keeps no permission DB
    fn retire_legacy_permission_schema(conn: &Connection) -> Result<()> {
        const LEGACY_PERMISSION_TABLE: &str = "room_permissions";

        let has_permission_table: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master
                           WHERE type = 'table' AND name = ?1)",
            params![LEGACY_PERMISSION_TABLE],
            |row| row.get(0),
        )?;
        if has_permission_table {
            let rows: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM {LEGACY_PERMISSION_TABLE}"),
                [],
                |row| row.get(0),
            )?;
            if rows > 0 {
                return Err(MaitreDError::LegacyRoomPermissions(rows as usize));
            }
            conn.execute_batch(&format!("DROP TABLE {LEGACY_PERMISSION_TABLE}"))?;
        }

        let has_done_column: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('rooms') WHERE name = 'done')",
            [],
            |row| row.get(0),
        )?;
        if has_done_column {
            let public: i64 =
                conn.query_row("SELECT COUNT(*) FROM rooms WHERE done <> 0", [], |row| {
                    row.get(0)
                })?;
            if public > 0 {
                return Err(MaitreDError::LegacyRoomDone(public as usize));
            }
            conn.execute_batch("ALTER TABLE rooms DROP COLUMN done")?;
        }

        Ok(())
    }

    /// Looks up an account by its salted `sub` hash. If it does not exist, inserts a new account.
    ///
    /// The caller hashes the Google `sub` first ([`crate::crypto::hash_sub`]);
    /// the raw identifier never reaches the database.
    ///
    /// spec: alhalte-maitre-d/accounts 0002 accounts are keyed by sub_hash
    pub fn get_or_create_account(
        &self,
        sub_hash: &SubHash,
        default_handle_name: &str,
        icon_tin_id: Option<[u8; 32]>,
        now: i64,
    ) -> Result<Account> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE sub_hash = ?1 LIMIT 1"
        ))?;

        let existing = stmt
            .query_row(params![sub_hash.as_hex()], account_from_row)
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
            "INSERT INTO accounts (account_id, sub_hash, handle_name, icon_tin_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                next_id,
                sub_hash.as_hex(),
                default_handle_name,
                icon_bytes,
                now
            ],
        )?;

        Ok(Account {
            account_id: next_id as u64,
            sub_hash: *sub_hash,
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

        let mut stmt = conn.prepare(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE account_id = ?1"
        ))?;

        let account = stmt
            .query_row(params![account_id as i64], account_from_row)
            .optional()?;

        Ok(account)
    }

    /// Retrieves an account by the salted hash of its Google `sub`.
    ///
    /// The argument is the hash, not the raw identifier: `maitre-d` cannot
    /// reverse it.
    pub fn get_account_by_sub(&self, sub_hash: &SubHash) -> Result<Option<Account>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE sub_hash = ?1 LIMIT 1"
        ))?;

        let account = stmt
            .query_row(params![sub_hash.as_hex()], account_from_row)
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

    /// Stores a single-use Google OAuth state bound to its post-login return URL.
    ///
    /// Expired rows are pruned opportunistically so a login cannot grow the
    /// table without bound. `ttl_secs` also bounds how long a state may be
    /// presented to the callback.
    pub fn create_oauth_state(
        &self,
        state: &str,
        return_to: &str,
        now: i64,
        ttl_secs: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        conn.execute(
            "DELETE FROM oauth_states WHERE created_at < ?1",
            params![now - ttl_secs],
        )?;
        conn.execute(
            "INSERT INTO oauth_states (state, return_to, created_at) VALUES (?1, ?2, ?3)",
            params![state, return_to, now],
        )?;

        Ok(())
    }

    /// Consumes a pending OAuth state and returns its bound `return_to`.
    ///
    /// The row is deleted in the same transaction that reads it, so a state is
    /// single-use even when it has already expired. Returns `None` for unknown,
    /// already-used, or expired states.
    pub fn consume_oauth_state(
        &self,
        state: &str,
        now: i64,
        ttl_secs: i64,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let tx = conn.unchecked_transaction()?;
        let row: Option<(String, i64)> = tx
            .query_row(
                "SELECT return_to, created_at FROM oauth_states WHERE state = ?1",
                params![state],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        tx.execute("DELETE FROM oauth_states WHERE state = ?1", params![state])?;
        tx.commit()?;

        match row {
            Some((return_to, created_at)) if created_at >= now - ttl_secs => Ok(Some(return_to)),
            _ => Ok(None),
        }
    }

    /// Registers a room in the registry.
    ///
    /// The caller is responsible for supplying a canonical 16-hex `room_id` and
    /// a validated `name`; the HTTP layer performs both checks. No permission
    /// row is written: the creator becomes the room's initial `Admin` when
    /// `alhalte-banquette` starts the room (see `Room::new`).
    ///
    /// # Errors
    ///
    /// Returns [`MaitreDError::RoomAlreadyExists`] when the room ID is already
    /// registered.
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

        conn.execute(
            "INSERT INTO rooms (room_id, name, creator_account, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![room_id, name, creator_account as i64, now],
        )?;

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
            "SELECT room_id, name, creator_account, created_at
             FROM rooms WHERE room_id = ?1",
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

    /// Retrieves the persisted `RoomData` head tin for a room.
    ///
    /// The `room_heads` row is external metadata: the `RoomData` wire schema is
    /// unchanged. `None` means the room has never been persisted and a new room
    /// may be started.
    pub fn get_room_head(&self, room_id: &str) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let mut stmt = conn.prepare("SELECT tin_id FROM room_heads WHERE room_id = ?1")?;
        let blob: Option<Vec<u8>> = stmt
            .query_row(params![room_id], |row| row.get(0))
            .optional()?;

        match blob {
            None => Ok(None),
            Some(bytes) => {
                let len = bytes.len();
                bytes.try_into().map(Some).map_err(|_| {
                    MaitreDError::Database(rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Blob,
                        Box::new(std::io::Error::other(format!(
                            "room_heads.tin_id for {room_id} must be 32 bytes, got {len}"
                        ))),
                    ))
                })
            }
        }
    }

    /// Stores or replaces the persisted `RoomData` head tin for a room.
    ///
    /// Returns the previously recorded head tin, if any. Callers may use it to
    /// reclaim the superseded head from cellar; the banquette process performs
    /// that best-effort unregister itself for the normal path.
    pub fn set_room_head(
        &self,
        room_id: &str,
        tin_id: &[u8; 32],
        now: i64,
    ) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().map_err(|e| {
            MaitreDError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(e.to_string()),
            )))
        })?;

        let previous: Option<Vec<u8>> = conn
            .query_row(
                "SELECT tin_id FROM room_heads WHERE room_id = ?1",
                params![room_id],
                |row| row.get(0),
            )
            .optional()?;

        conn.execute(
            "INSERT INTO room_heads (room_id, tin_id, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (room_id) DO UPDATE SET tin_id = excluded.tin_id,
                                                 updated_at = excluded.updated_at",
            params![room_id, tin_id.as_slice(), now],
        )?;

        match previous {
            None => Ok(None),
            Some(bytes) => {
                if bytes.len() == 32 {
                    let mut out = [0u8; 32];
                    out.copy_from_slice(&bytes);
                    Ok(Some(out))
                } else {
                    // A corrupt legacy row is replaced by the new head; report no
                    // usable previous head rather than failing the write.
                    Ok(None)
                }
            }
        }
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
                stmt = conn.prepare(&format!(
                    "SELECT {ACCOUNT_COLUMNS}
                     FROM accounts
                     WHERE account_id = ?1 OR handle_name LIKE ?2
                     ORDER BY (account_id = ?1) DESC, account_id ASC
                     LIMIT 50"
                ))?;
                stmt.query(params![id, like_pattern])?
            }
            None => {
                stmt = conn.prepare(&format!(
                    "SELECT {ACCOUNT_COLUMNS}
                     FROM accounts
                     WHERE handle_name LIKE ?1
                     ORDER BY account_id ASC
                     LIMIT 50"
                ))?;
                stmt.query(params![like_pattern])?
            }
        };

        let mut accounts = Vec::new();
        while let Some(row) = rows.next()? {
            accounts.push(account_from_row(row)?);
        }

        Ok(accounts)
    }
}
