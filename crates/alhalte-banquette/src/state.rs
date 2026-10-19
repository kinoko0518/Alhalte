//! Scene state management with size enforcement and namespace validation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use tin::Tin;

use crate::error::{BanquetteError, Result};
use crate::labels::KEY_STD_ADMINS;

/// Maximum allowable serialized plaintext size of a single `Tin` stored in scene state (4KiB).
pub const MAX_STATE_VALUE_SIZE: usize = 4096;

/// Validates that a state key follows the required namespace convention.
///
/// Keys are expected to use one or more `::` separators (e.g. `std::name`, `alhalte::grid`).
pub fn validate_state_key(key: &str) -> Result<()> {
    if !key.contains("::") {
        return Err(BanquetteError::InvalidStateKey(key.to_string()));
    }
    Ok(())
}

/// Checks whether a key resides in the reserved standard namespace (`std::*`).
pub fn is_reserved_key(key: &str) -> bool {
    key.starts_with("std::")
}

/// Checks that a `Tin` value does not exceed the 4KiB limit.
pub fn validate_state_value(value: &Tin) -> Result<()> {
    let size = value.to_plaintext().len();
    if size > MAX_STATE_VALUE_SIZE {
        return Err(BanquetteError::StateValueTooLarge {
            size,
            max: MAX_STATE_VALUE_SIZE,
        });
    }
    Ok(())
}

/// Scene state map backed by a sorted [`BTreeMap<String, Tin>`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneState {
    entries: BTreeMap<String, Tin>,
}

impl SceneState {
    /// Creates a new empty [`SceneState`].
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Creates a [`SceneState`] initialized with existing entries.
    pub fn from_entries(entries: BTreeMap<String, Tin>) -> Result<Self> {
        for (k, v) in &entries {
            validate_state_key(k)?;
            validate_state_value(v)?;
        }
        Ok(Self { entries })
    }

    /// Retrieves a reference to the underlying [`BTreeMap`].
    pub const fn as_map(&self) -> &BTreeMap<String, Tin> {
        &self.entries
    }

    /// Consumes this [`SceneState`] into the underlying [`BTreeMap`].
    pub fn into_inner(self) -> BTreeMap<String, Tin> {
        self.entries
    }

    /// Retrieves a value by key.
    pub fn get(&self, key: &str) -> Option<&Tin> {
        self.entries.get(key)
    }

    /// Sets or updates a state entry with size and namespace validation.
    ///
    /// If the key is `std::admins`, `is_admin` must be `true`.
    pub fn update(&mut self, key: String, value: Tin, is_admin: bool) -> Result<Option<Tin>> {
        validate_state_key(&key)?;
        validate_state_value(&value)?;

        if key == KEY_STD_ADMINS && !is_admin {
            return Err(BanquetteError::ReservedKeyPermissionDenied(key));
        }

        Ok(self.entries.insert(key, value))
    }

    /// Deletes a key from the state map.
    pub fn delete(&mut self, key: &str, is_admin: bool) -> Result<Option<Tin>> {
        if key == KEY_STD_ADMINS && !is_admin {
            return Err(BanquetteError::ReservedKeyPermissionDenied(key.to_string()));
        }

        Ok(self.entries.remove(key))
    }

    /// Returns the number of entries in the state map.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Checks if the state map is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
