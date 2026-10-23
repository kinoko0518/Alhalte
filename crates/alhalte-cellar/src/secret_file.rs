//! Reading secrets from owner-only files (Issue #194).
//!
//! Secrets are injected as files instead of command-line arguments so they never
//! appear in `ps` output or in a generated systemd unit. alhalte-restaurant writes them with
//! mode `0600` under `<data-dir>/secrets/`; a file that is accessible by group or
//! others is rejected rather than silently accepted.

use std::path::{Path, PathBuf};

/// Errors that can occur while reading a secret file.
#[derive(Debug, thiserror::Error)]
pub enum SecretFileError {
    /// The file could not be read.
    #[error("failed to read secret file {path}: {source}")]
    Io {
        /// Path that was read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The file is accessible by group or others.
    #[error("secret file {path} has mode {mode:04o}; owner-only (0600) permissions are required")]
    Permissions {
        /// Path that was read.
        path: PathBuf,
        /// Observed permission bits.
        mode: u32,
    },

    /// The file contains no secret.
    #[error("secret file {path} is empty")]
    Empty {
        /// Path that was read.
        path: PathBuf,
    },
}

/// Reads a secret from `path`, rejecting files readable by group or others.
///
/// Trailing line endings are stripped so `printf secret > file` and
/// `echo secret > file` behave the same.
///
/// # Errors
///
/// Returns [`SecretFileError::Permissions`] when the file is not owner-only,
/// [`SecretFileError::Empty`] when it holds no secret, and
/// [`SecretFileError::Io`] when it cannot be read.
pub fn read_secret_file(path: impl AsRef<Path>) -> Result<String, SecretFileError> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path).map_err(|source| SecretFileError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path).map_err(|source| SecretFileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(SecretFileError::Permissions {
                path: path.to_path_buf(),
                mode,
            });
        }
    }

    let secret = content.trim_end_matches(['\n', '\r']).to_string();
    if secret.is_empty() {
        return Err(SecretFileError::Empty {
            path: path.to_path_buf(),
        });
    }
    Ok(secret)
}

/// Resolves a secret from a file (preferred) or an inline value.
///
/// The file always wins so a stale `--service-token` value cannot override the
/// injected secret. Empty inline values are treated as unset.
///
/// # Errors
///
/// Returns [`SecretFileError`] when the file cannot be read.
pub fn resolve_secret(
    inline_file: Option<&Path>,
    inline_value: Option<&str>,
) -> Result<Option<String>, SecretFileError> {
    if let Some(path) = inline_file {
        return read_secret_file(path).map(Some);
    }
    Ok(inline_value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: alhalte-cellar/cli 0007 secrets are read from owner-only files
    #[test]
    fn reads_owner_only_secret_file() {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(b"file-secret\n").unwrap();
        drop(file);

        assert_eq!(read_secret_file(&path).unwrap(), "file-secret");
        // The file wins over an inline value.
        assert_eq!(
            resolve_secret(Some(&path), Some("inline-secret")).unwrap(),
            Some("file-secret".to_string())
        );

        // Group/other-readable files are rejected.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            read_secret_file(&path),
            Err(SecretFileError::Permissions { mode: 0o644, .. })
        ));

        // Empty files are rejected.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&path, b"").unwrap();
        assert!(matches!(
            read_secret_file(&path),
            Err(SecretFileError::Empty { .. })
        ));
    }

    #[test]
    fn inline_value_is_the_fallback() {
        assert_eq!(
            resolve_secret(None, Some(" inline-secret ")).unwrap(),
            Some(" inline-secret ".to_string())
        );
        assert_eq!(resolve_secret(None, Some("   ")).unwrap(), None);
        assert_eq!(resolve_secret(None, None).unwrap(), None);
    }
}
