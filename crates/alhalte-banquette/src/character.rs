//! Character definition, biblio map, and attribute helper functions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use tin::Tin;

use crate::error::{BanquetteError, Result};
use crate::labels::{KEY_STD_NAME, KEY_STD_OWNER_ACCOUNT, LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID};

/// A character representation in the Alhalte ecosystem, mapping attribute keys to self-typed `Tin` containers.
pub type Character = HashMap<String, Tin>;

/// Extracts the human-readable name from `std::name` attribute if present.
pub fn get_character_name(character: &Character) -> Option<String> {
    character
        .get(KEY_STD_NAME)
        .and_then(|t| t.as_string().ok())
        .map(ToString::to_string)
}

/// Sets the human-readable name in `std::name` attribute (`primitive/string`).
pub fn set_character_name(character: &mut Character, name: &str) {
    character.insert(KEY_STD_NAME.to_string(), Tin::from_string(name));
}

/// Extracts the owner account ID from `std::owner_account` attribute if present.
pub fn get_character_owner(character: &Character) -> Option<u64> {
    let tin = character.get(KEY_STD_OWNER_ACCOUNT)?;
    if tin.label != LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID && tin.label != tin::LABEL_PRIMITIVE_U64 {
        return None;
    }
    if tin.payload.len() != 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&tin.payload);
    Some(u64::from_le_bytes(buf))
}

/// Sets the owner account ID in `std::owner_account` attribute (`alhalte/maitre-d/account-id`).
pub fn set_character_owner(character: &mut Character, owner: u64) {
    character.insert(
        KEY_STD_OWNER_ACCOUNT.to_string(),
        Tin::new(
            LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID,
            owner.to_le_bytes().to_vec(),
        ),
    );
}

/// Character repository (`character-biblio`) managing unique character ID allocation and data storage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterBiblio {
    next_id: u64,
    characters: HashMap<u64, Character>,
}

impl CharacterBiblio {
    /// Creates a new empty [`CharacterBiblio`].
    pub fn new() -> Self {
        Self {
            next_id: 1,
            characters: HashMap::new(),
        }
    }

    /// Allocates a unique ID and registers a new character.
    ///
    /// If `std::owner_account` is not set on `initial_data`, it is populated with `creator_account`.
    pub fn create_character(&mut self, creator_account: u64, mut initial_data: Character) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);

        if get_character_owner(&initial_data).is_none() {
            set_character_owner(&mut initial_data, creator_account);
        }

        self.characters.insert(id, initial_data);
        id
    }

    /// Retrieves a character by ID.
    pub fn get(&self, id: u64) -> Option<&Character> {
        self.characters.get(&id)
    }

    /// Retrieves a mutable reference to a character by ID.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Character> {
        self.characters.get_mut(&id)
    }

    /// Updates a character. Only the character's owner or a room administrator can perform updates.
    pub fn update(
        &mut self,
        id: u64,
        caller_account: u64,
        is_admin: bool,
        mut new_data: Character,
    ) -> Result<()> {
        let existing = self
            .characters
            .get(&id)
            .ok_or(BanquetteError::CharacterNotFound(id))?;

        let owner = get_character_owner(existing).unwrap_or(caller_account);
        if !is_admin && caller_account != owner {
            return Err(BanquetteError::PermissionDenied);
        }

        // Preserve owner if not explicitly changed by admin
        if !is_admin || get_character_owner(&new_data).is_none() {
            set_character_owner(&mut new_data, owner);
        }

        self.characters.insert(id, new_data);
        Ok(())
    }

    /// Deletes a character by ID. Only the character's owner or a room administrator can delete it.
    pub fn delete(&mut self, id: u64, caller_account: u64, is_admin: bool) -> Result<Character> {
        let existing = self
            .characters
            .get(&id)
            .ok_or(BanquetteError::CharacterNotFound(id))?;

        let owner = get_character_owner(existing).unwrap_or(caller_account);
        if !is_admin && caller_account != owner {
            return Err(BanquetteError::PermissionDenied);
        }

        self.characters
            .remove(&id)
            .ok_or(BanquetteError::CharacterNotFound(id))
    }

    /// Returns a reference to all characters.
    pub const fn all(&self) -> &HashMap<u64, Character> {
        &self.characters
    }

    /// Returns the number of characters in the biblio.
    pub fn len(&self) -> usize {
        self.characters.len()
    }

    /// Checks if the biblio is empty.
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }
}
