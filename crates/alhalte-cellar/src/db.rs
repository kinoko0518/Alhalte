//! SQLite database management for storing tin references.
//!
//! According to ARCHITECTURE.md:
//! References are stored as the Cartesian product of:
//! `(tin_id: [u8; 32], path: String, owner: u64, wrapped_key: Option<Vec<u8>>)`.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use tin::TinId;

use crate::error::Result;
use crate::model::TinReference;

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

    /// Initializes the database tables and indices if not already present.
    pub fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tin_references (
                tin_id BLOB NOT NULL,
                path TEXT NOT NULL,
                owner INTEGER NOT NULL,
                wrapped_key BLOB,
                PRIMARY KEY (tin_id, owner)
            );
            CREATE INDEX IF NOT EXISTS idx_tin_references_path ON tin_references(path);
            CREATE INDEX IF NOT EXISTS idx_tin_references_tin_id ON tin_references(tin_id);",
        )?;
        Ok(())
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
        path: &str,
        owner: u64,
        wrapped_key: Option<&[u8]>,
    ) -> Result<()> {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO tin_references (tin_id, path, owner, wrapped_key)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (tin_id, owner) DO UPDATE SET
                 path = excluded.path,
                 wrapped_key = COALESCE(excluded.wrapped_key, tin_references.wrapped_key)",
        )?;
        stmt.execute(params![tin_id.as_slice(), path, owner as i64, wrapped_key])?;
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
                "SELECT tin_id, path, owner, wrapped_key FROM tin_references WHERE tin_id = ?1 AND owner = ?2 LIMIT 1"
            }
            None => {
                "SELECT tin_id, path, owner, wrapped_key FROM tin_references WHERE tin_id = ?1 LIMIT 1"
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

        let path: String = row.get(1)?;
        let owner_i64: i64 = row.get(2)?;
        let wrapped_key: Option<Vec<u8>> = row.get(3)?;

        Ok(TinReference {
            tin_id,
            path,
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

    /// Returns the set of all unique paths referenced in the database.
    pub fn get_all_referenced_paths(conn: &Connection) -> Result<HashSet<String>> {
        let mut stmt = conn.prepare_cached("SELECT DISTINCT path FROM tin_references")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut paths = HashSet::new();
        for path_res in rows {
            paths.insert(path_res?);
        }
        Ok(paths)
    }

    /// Returns the set of all unique tin IDs referenced in the database.
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
}
