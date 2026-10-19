//! Garbage collection (GC) for unreferenced tin entities in storage.

use std::collections::HashSet;

use rusqlite::Connection;
use tin::TinId;

use crate::db::Database;
use crate::error::Result;
use crate::model::GcStats;
use crate::storage::StorageManager;

/// Runs garbage collection over the storage directory.
///
/// An entity file is named `hex(tin_id)` on disk (see
/// [`StorageManager::relative_name`]), so the referenced set is built by
/// hashing the distinct `tin_id` rows instead of reading a stored path column.
///
/// Both reference kinds count: `tin_references` rows **and** `aliases`
/// pointers. A room head that only the `room/<room_id>/head` alias points at
/// is therefore never collected (Issue #192).
///
/// Any file that neither kind references is permanently removed.
pub fn run_garbage_collection(storage: &StorageManager, conn: &Connection) -> Result<GcStats> {
    let files = storage.list_tin_files()?;
    let mut referenced: HashSet<TinId> = Database::get_all_referenced_tin_ids(conn)?;
    referenced.extend(Database::get_all_alias_tin_ids(conn)?);
    let referenced_names: HashSet<String> = referenced
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
