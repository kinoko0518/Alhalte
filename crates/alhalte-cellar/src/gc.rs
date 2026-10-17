//! Garbage collection (GC) for unreferenced tin entities in storage.

use rusqlite::Connection;

use crate::db::Database;
use crate::error::Result;
use crate::model::GcStats;
use crate::storage::StorageManager;

/// Runs garbage collection over the storage directory.
///
/// Any file in the storage directory that is not referenced by at least one row
/// in the SQLite database is permanently removed.
pub fn run_garbage_collection(storage: &StorageManager, conn: &Connection) -> Result<GcStats> {
    let files = storage.list_tin_files()?;
    let mut stats = GcStats {
        scanned_files: files.len(),
        deleted_files: 0,
        freed_bytes: 0,
    };

    for (tin_id, _path, size) in files {
        let is_referenced = Database::has_tin_reference(conn, &tin_id)?;
        if !is_referenced {
            storage.delete_tin(&tin_id)?;
            stats.deleted_files += 1;
            stats.freed_bytes += size;
        }
    }

    Ok(stats)
}
