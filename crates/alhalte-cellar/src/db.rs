//! SQLite database management for storing tin references and aliases.
//!
//! According to ARCHITECTURE.md:
//! References are stored as the Cartesian product of:
//! `(tin_id: [u8; 32], owner: u64, wrapped_key: Option<Vec<u8>>)`.
//!
//! The entity file name is not stored: it is always `hex(tin_id)` (see
//! [`crate::storage::StorageManager::relative_name`]).
//!
//! [`Database::get_alias`] / [`Database::set_alias`] / [`Database::delete_alias`]
//! back the generic `name -> tin_id` pointer table used for room heads
//! (`room/<room_id>/head`). An alias is a reference too: GC keeps a tin that an
//! alias points at even when no `tin_references` row remains.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use tin::TinId;

use crate::error::{CellarError, Result};
use crate::model::TinReference;

/// Table holding ownership references to stored tin entities.
const TIN_REFERENCES_TABLE: &str = "tin_references";
/// Legacy index on the removed `path` column.
const LEGACY_PATH_INDEX: &str = "idx_tin_references_path";
/// Legacy column that duplicated `hex(tin_id)`.
const LEGACY_PATH_COLUMN: &str = "path";

/// Database connection and operations for `alhalte-cellar`.
pub struct Database;

impl Database {
    /// Opens or creates the SQLite database at `db_path` and initializes the schema.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Connection> {
        let path = db_path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // Performance pragmas
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        Self::init_schema(&conn)?;

        Ok(conn)
    }

    /// Initializes schema in an in-memory SQLite database (for tests).
    pub fn open_in_memory() -> Result<Connection> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(conn)
    }

    /// Initializes the database tables and indices, migrating any legacy schema.
    pub fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tin_references (
                tin_id BLOB NOT NULL,
                owner INTEGER NOT NULL,
                wrapped_key BLOB,
                PRIMARY KEY (tin_id, owner)
            );
            CREATE INDEX IF NOT EXISTS idx_tin_references_tin_id ON tin_references(tin_id);
            CREATE TABLE IF NOT EXISTS aliases (
                name TEXT PRIMARY KEY,
                tin_id BLOB NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )?;
        Self::migrate_drop_path_column(conn)?;
        Ok(())
    }

    /// Drops the legacy `path` column and its index when they are still present.
    ///
    /// `path` always duplicated `hex(tin_id)`, so dropping it loses no
    /// information: `tin_id`, `owner` and `wrapped_key` are left untouched.
    /// SQLite refuses to drop a column that an index still references, so the
    /// legacy index goes first. Both statements run in one transaction, so a
    /// failure keeps the old schema and every reference row intact.
    fn migrate_drop_path_column(conn: &Connection) -> Result<()> {
        if !Self::has_column(conn, TIN_REFERENCES_TABLE, LEGACY_PATH_COLUMN)? {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(&format!(
            "DROP INDEX IF EXISTS {LEGACY_PATH_INDEX};
             ALTER TABLE {TIN_REFERENCES_TABLE} DROP COLUMN {LEGACY_PATH_COLUMN};",
        ))?;
        tx.commit()?;
        Ok(())
    }

    /// Returns whether `table` currently exposes a column named `column`.
    fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name.eq_ignore_ascii_case(column) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Checks if a reference for `(tin_id, owner)` already exists.
    pub fn has_owner_reference(conn: &Connection, tin_id: &TinId, owner: u64) -> Result<bool> {
        let mut stmt = conn.prepare_cached(
            "SELECT 1 FROM tin_references WHERE tin_id = ?1 AND owner = ?2 LIMIT 1",
        )?;
        let exists = stmt
            .query_row(params![tin_id.as_slice(), owner as i64], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        Ok(exists)
    }

    /// Checks if any reference for `tin_id` exists in the database.
    pub fn has_tin_reference(conn: &Connection, tin_id: &TinId) -> Result<bool> {
        let mut stmt =
            conn.prepare_cached("SELECT 1 FROM tin_references WHERE tin_id = ?1 LIMIT 1")?;
        let exists = stmt
            .query_row(params![tin_id.as_slice()], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        Ok(exists)
    }

    /// Inserts or updates a reference for `(tin_id, owner)`.
    pub fn insert_reference(
        conn: &Connection,
        tin_id: &TinId,
        owner: u64,
        wrapped_key: Option<&[u8]>,
    ) -> Result<()> {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO tin_references (tin_id, owner, wrapped_key)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (tin_id, owner) DO UPDATE SET
                 wrapped_key = COALESCE(excluded.wrapped_key, tin_references.wrapped_key)",
        )?;
        stmt.execute(params![tin_id.as_slice(), owner as i64, wrapped_key])?;
        Ok(())
    }

    /// Retrieves the reference row for a given `tin_id`, optionally filtered by `owner`.
    pub fn get_reference(
        conn: &Connection,
        tin_id: &TinId,
        owner: Option<u64>,
    ) -> Result<Option<TinReference>> {
        let sql = match owner {
            Some(_) => {
                "SELECT tin_id, owner, wrapped_key FROM tin_references WHERE tin_id = ?1 AND owner = ?2 LIMIT 1"
            }
            None => {
                "SELECT tin_id, owner, wrapped_key FROM tin_references WHERE tin_id = ?1 LIMIT 1"
            }
        };

        let mut stmt = conn.prepare_cached(sql)?;
        let query_result = match owner {
            Some(o) => stmt.query_row(params![tin_id.as_slice(), o as i64], Self::map_row),
            None => stmt.query_row(params![tin_id.as_slice()], Self::map_row),
        };

        query_result.optional().map_err(Into::into)
    }

    /// Helper to map a row to `TinReference`.
    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TinReference> {
        let tin_id_blob: Vec<u8> = row.get(0)?;
        if tin_id_blob.len() != 32 {
            return Err(rusqlite::Error::InvalidColumnType(
                0,
                "tin_id".to_string(),
                rusqlite::types::Type::Blob,
            ));
        }
        let mut tin_id = [0u8; 32];
        tin_id.copy_from_slice(&tin_id_blob);

        let owner_i64: i64 = row.get(1)?;
        let wrapped_key: Option<Vec<u8>> = row.get(2)?;

        Ok(TinReference {
            tin_id,
            owner: owner_i64 as u64,
            wrapped_key,
        })
    }

    /// Deletes the ownership reference for `(tin_id, owner)`.
    ///
    /// Returns `true` if a row was found and removed, `false` otherwise.
    pub fn delete_reference(conn: &Connection, tin_id: &TinId, owner: u64) -> Result<bool> {
        let mut stmt =
            conn.prepare_cached("DELETE FROM tin_references WHERE tin_id = ?1 AND owner = ?2")?;
        let rows = stmt.execute(params![tin_id.as_slice(), owner as i64])?;
        Ok(rows > 0)
    }

    /// Counts how many references remain for `tin_id`.
    pub fn count_references_for_tin(conn: &Connection, tin_id: &TinId) -> Result<usize> {
        let mut stmt =
            conn.prepare_cached("SELECT COUNT(*) FROM tin_references WHERE tin_id = ?1")?;
        let count: i64 = stmt.query_row(params![tin_id.as_slice()], |r| r.get(0))?;
        Ok(count as usize)
    }

    /// Returns the set of all unique tin IDs referenced in the database.
    ///
    /// The corresponding entity file name is `hex(tin_id)` (see
    /// [`crate::storage::StorageManager::relative_name`]); GC derives it from
    /// this set because no path is persisted.
    ///
    /// Only `tin_references` rows are returned. Alias pointers are a second
    /// kind of reference and are returned by [`Self::get_all_alias_tin_ids`];
    /// GC unions both sets before deleting files.
    pub fn get_all_referenced_tin_ids(conn: &Connection) -> Result<HashSet<TinId>> {
        let mut stmt = conn.prepare_cached("SELECT DISTINCT tin_id FROM tin_references")?;
        let rows = stmt.query_map([], |r| {
            let blob: Vec<u8> = r.get(0)?;
            Ok(blob)
        })?;
        let mut ids = HashSet::new();
        for blob_res in rows {
            let blob = blob_res?;
            if blob.len() == 32 {
                let mut id = [0u8; 32];
                id.copy_from_slice(&blob);
                ids.insert(id);
            }
        }
        Ok(ids)
    }

    /// Reads the tin ID stored under the generic alias `name`.
    ///
    /// Returns `None` when the name was never set (or was deleted). The stored
    /// column is a plain 32-byte blob; a row of another length is a corrupt
    /// database and reported as an error instead of being ignored.
    pub fn get_alias(conn: &Connection, name: &str) -> Result<Option<TinId>> {
        let mut stmt = conn.prepare_cached("SELECT tin_id FROM aliases WHERE name = ?1")?;
        let blob: Option<Vec<u8>> = stmt.query_row(params![name], |row| row.get(0)).optional()?;

        blob.map(|bytes| Self::tin_id_from_alias_blob(name, bytes))
            .transpose()
    }

    /// Points the generic alias `name` at `tin_id`.
    ///
    /// Returns the tin ID the alias pointed at before the write, if any.
    pub fn set_alias(
        conn: &Connection,
        name: &str,
        tin_id: &TinId,
        updated_at: i64,
    ) -> Result<Option<TinId>> {
        let previous = Self::get_alias(conn, name)?;
        let mut stmt = conn.prepare_cached(
            "INSERT INTO aliases (name, tin_id, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (name) DO UPDATE SET
                 tin_id = excluded.tin_id,
                 updated_at = excluded.updated_at",
        )?;
        stmt.execute(params![name, tin_id.as_slice(), updated_at])?;
        Ok(previous)
    }

    /// Deletes the generic alias `name`.
    ///
    /// Returns `true` when a row was removed and `false` when the name was
    /// already unused.
    pub fn delete_alias(conn: &Connection, name: &str) -> Result<bool> {
        let mut stmt = conn.prepare_cached("DELETE FROM aliases WHERE name = ?1")?;
        let rows = stmt.execute(params![name])?;
        Ok(rows > 0)
    }

    /// Returns the set of every tin ID an alias currently points at.
    ///
    /// GC treats these as references exactly like `tin_references` rows, so a
    /// tin that only a room-head alias points at is never collected.
    pub fn get_all_alias_tin_ids(conn: &Connection) -> Result<HashSet<TinId>> {
        let mut stmt = conn.prepare_cached("SELECT name, tin_id FROM aliases")?;
        let rows = stmt.query_map([], |r| {
            let name: String = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            Ok((name, blob))
        })?;
        let mut ids = HashSet::new();
        for row in rows {
            let (name, blob) = row?;
            // A malformed row would otherwise make GC delete the tin it points
            // at, so it is surfaced instead of skipped.
            ids.insert(Self::tin_id_from_alias_blob(&name, blob)?);
        }
        Ok(ids)
    }

    /// Parses an `aliases.tin_id` blob, naming the alias in the error message.
    fn tin_id_from_alias_blob(name: &str, blob: Vec<u8>) -> Result<TinId> {
        let len = blob.len();
        blob.try_into().map_err(|_| {
            CellarError::Internal(format!(
                "aliases.tin_id for {name} must be 32 bytes, got {len}"
            ))
        })
    }
}
