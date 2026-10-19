//! Garbage collection (GC) for unreferenced tin entities in storage.

use std::collections::HashSet;

use rusqlite::Connection;

use crate::db::Database;
use crate::error::Result;
use crate::model::GcStats;
use crate::storage::StorageManager;

/// Runs garbage collection over the storage directory.
///
/// An entity file is named `hex(tin_id)` on disk (see
/// [`StorageManager::relative_name`]), so the referenced set is built by
/// hashing the distinct `tin_id` rows instead of reading a stored path column.
/// Any file that no row references is permanently removed.
pub fn run_garbage_collection(storage: &StorageManager, conn: &Connection) -> Result<GcStats> {
    let files = storage.list_tin_files()?;
    let referenced_names: HashSet<String> = Database::get_all_referenced_tin_ids(conn)?
        .iter()
        .map(StorageManager::relative_name)
        .collect();
    let mut stats = GcStats {
        scanned_files: files.len(),
        deleted_files: 0,
        freed_bytes: 0,
    };

    for (tin_id, _path, size) in files {
        let name = StorageManager::relative_name(&tin_id);
        if !referenced_names.contains(&name) {
            storage.delete_tin(&tin_id)?;
            stats.deleted_files += 1;
            stats.freed_bytes += size;
        }
    }

    Ok(stats)
}
