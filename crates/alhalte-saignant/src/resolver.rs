//! Character identity and avatar resolution abstractions.

use std::collections::HashMap;

/// Trait for resolving character identity (name and avatar URL) from a character ID.
pub trait CharacterResolver {
    /// Resolves character ID to display name. Returns `None` if the character is not found.
    fn resolve_name(&self, character_id: u64) -> Option<String>;

    /// Resolves character ID to avatar image URL. Returns `None` if unknown or unavailable.
    fn resolve_avatar_url(&self, _character_id: u64) -> Option<String> {
        None
    }
}

/// Fallback resolver that returns `None` for all queries.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopCharacterResolver;

impl CharacterResolver for NoopCharacterResolver {
    fn resolve_name(&self, _character_id: u64) -> Option<String> {
        None
    }
}

impl<F> CharacterResolver for F
where
    F: Fn(u64) -> Option<String>,
{
    fn resolve_name(&self, character_id: u64) -> Option<String> {
        self(character_id)
    }
}

impl CharacterResolver for HashMap<u64, String> {
    fn resolve_name(&self, character_id: u64) -> Option<String> {
        self.get(&character_id).cloned()
    }
}

impl CharacterResolver for HashMap<u64, (String, Option<String>)> {
    fn resolve_name(&self, character_id: u64) -> Option<String> {
        self.get(&character_id).map(|(name, _)| name.clone())
    }

    fn resolve_avatar_url(&self, character_id: u64) -> Option<String> {
        self.get(&character_id)
            .and_then(|(_, avatar)| avatar.clone())
    }
}
