//! Thread-safe scoped environment-variable manipulation.
//!
//! Under Rust 2024 edition, environment variable modification is unsafe. This
//! module provides the single serialized boundary used by scoped `ROPS_AGE`
//! replacement. Unix uses POSIX `setenv`/`unsetenv`; other platforms use the
//! standard library calls while holding the same mutex.

#[cfg(unix)]
use std::ffi::CString;
use std::sync::Mutex;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Safely runs a closure with a temporary environment variable set.
///
/// Restores the previous value (or unsets it if it was not present) upon completion.
pub fn with_scoped_env_var<T>(key: &str, value: &str, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var(key).ok();

    set_env_raw(key, value);
    let result = f();

    if let Some(prev_val) = previous {
        set_env_raw(key, &prev_val);
    } else {
        unset_env_raw(key);
    }

    result
}

/// Sets an environment variable on Unix via POSIX FFI.
#[cfg(unix)]
fn set_env_raw(key: &str, value: &str) {
    if let (Ok(c_key), Ok(c_val)) = (CString::new(key), CString::new(value)) {
        #[allow(unsafe_code)]
        // SAFETY: Calling standard POSIX libc setenv with valid null-terminated strings.
        unsafe {
            libc::setenv(c_key.as_ptr(), c_val.as_ptr(), 1);
        }
    }
}

/// Unsets an environment variable on Unix via POSIX FFI.
#[cfg(unix)]
fn unset_env_raw(key: &str) {
    if let Ok(c_key) = CString::new(key) {
        #[allow(unsafe_code)]
        // SAFETY: Calling standard POSIX libc unsetenv with valid null-terminated string.
        unsafe {
            libc::unsetenv(c_key.as_ptr());
        }
    }
}

/// Sets an environment variable on non-Unix platforms.
#[cfg(not(unix))]
fn set_env_raw(key: &str, value: &str) {
    #[allow(unsafe_code)]
    // SAFETY: Environment mutation is serialized by `ENV_MUTEX`, and this call
    // does not run concurrently with another environment read/write in this
    // process (the same contract the Unix path relies on).
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Unsets an environment variable on non-Unix platforms.
#[cfg(not(unix))]
fn unset_env_raw(key: &str) {
    #[allow(unsafe_code)]
    // SAFETY: Environment mutation is serialized by `ENV_MUTEX`.
    unsafe {
        std::env::remove_var(key);
    }
}
