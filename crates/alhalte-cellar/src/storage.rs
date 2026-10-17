//! File system storage manager for tin entity files.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use tin::TinId;

use crate::error::{CellarError, Result};
use crate::model::{format_tin_id, parse_tin_id};

/// Manages physical files in the storage directory.
#[derive(Debug, Clone)]
pub struct StorageManager {
    storage_path: PathBuf,
}

impl StorageManager {
    /// Creates a new `StorageManager` rooted at `storage_path`, creating directories if needed.
    pub fn new(storage_path: impl Into<PathBuf>) -> Result<Self> {
        let path = storage_path.into();
        fs::create_dir_all(&path)?;
        Ok(Self { storage_path: path })
    }

    /// Returns the root storage directory.
    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.storage_path
    }

    /// Returns the canonical relative file name for a tin ID.
    #[must_use]
    pub fn relative_name(tin_id: &TinId) -> String {
        format_tin_id(tin_id)
    }

    /// Returns the absolute path where the tin entity file is located.
    #[must_use]
    pub fn tin_file_path(&self, tin_id: &TinId) -> PathBuf {
        self.storage_path.join(Self::relative_name(tin_id))
    }

    /// Checks whether the tin entity file exists on disk.
    #[must_use]
    pub fn has_tin_file(&self, tin_id: &TinId) -> bool {
        self.tin_file_path(tin_id).is_file()
    }

    /// Writes tin data atomically to disk by writing to a temporary file and renaming it.
    pub fn write_tin(&self, tin_id: &TinId, data: &[u8]) -> Result<PathBuf> {
        let target_path = self.tin_file_path(tin_id);
        if target_path.is_file() {
            // Already present
            return Ok(target_path);
        }

        let rand_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_name = format!(
            ".tmp.{}.{}.{}",
            format_tin_id(tin_id),
            std::process::id(),
            rand_suffix
        );
        let temp_path = self.storage_path.join(temp_name);

        {
            let mut file = File::create(&temp_path)?;
            file.write_all(data)?;
            file.sync_all()?;
        }

        fs::rename(&temp_path, &target_path)?;
        Ok(target_path)
    }

    /// Reads the raw tin binary from disk.
    pub fn read_tin(&self, tin_id: &TinId) -> Result<Vec<u8>> {
        let path = self.tin_file_path(tin_id);
        if !path.is_file() {
            return Err(CellarError::NotFound(format_tin_id(tin_id)));
        }
        let data = fs::read(&path)?;
        Ok(data)
    }

    /// Deletes a tin entity file from disk.
    pub fn delete_tin(&self, tin_id: &TinId) -> Result<()> {
        let path = self.tin_file_path(tin_id);
        if path.is_file() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Lists all tin files currently residing in the storage directory.
    ///
    /// Returns a list of `(tin_id, absolute_path, file_size_in_bytes)`.
    pub fn list_tin_files(&self) -> Result<Vec<(TinId, PathBuf, u64)>> {
        let mut results = Vec::new();
        if !self.storage_path.is_dir() {
            return Ok(results);
        }

        for entry_res in fs::read_dir(&self.storage_path)? {
            let entry = entry_res?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                // Ignore temporary files starting with '.'
                if file_name.starts_with('.') {
                    continue;
                }

                if let Ok(tin_id) = parse_tin_id(file_name) {
                    let metadata = entry.metadata()?;
                    results.push((tin_id, path, metadata.len()));
                }
            }
        }

        Ok(results)
    }

    /// Explicit sink representing discarding received bytes into `/dev/null` for anti-oracle mode.
    pub fn discard_to_dev_null(_data: &[u8]) {
        // Discard data without writing to disk
    }
}
