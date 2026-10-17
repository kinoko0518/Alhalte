//! Serialized query representations for session logs.

use std::collections::HashMap;

use alhalte_saignant::LogEntry;

use crate::error::Result;
use crate::transpile::{EpubOptions, transpile_to_epub, transpile_to_html5};

/// An ordered sequence of scenes and their logs resulting from a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedSession {
    /// Document or session title.
    pub title: String,
    /// Ordered list of serialized scenes.
    pub scenes: Vec<SerializedScene>,
    /// Character name resolution dictionary mapping `character_id` to human-readable name.
    pub character_names: HashMap<u64, String>,
}

/// A serialized scene containing its scene identifier, title, and chronological log entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedScene {
    /// Scene identifier (e.g. `"prologue"`, `"tavern"`).
    pub scene_id: String,
    /// Human-readable scene title.
    pub title: String,
    /// Ordered log entries belonging to this scene.
    pub logs: Vec<LogEntry>,
}

impl SerializedSession {
    /// Creates a new serialized session with given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            scenes: Vec::new(),
            character_names: HashMap::new(),
        }
    }

    /// Adds a serialized scene to the session.
    pub fn add_scene(&mut self, scene: SerializedScene) {
        self.scenes.push(scene);
    }

    /// Returns the total number of log entries across all scenes in this session.
    pub fn total_log_count(&self) -> usize {
        self.scenes.iter().map(|s| s.logs.len()).sum()
    }

    /// Transpiles this session into a standalone HTML5 document.
    pub fn to_html5(&self) -> String {
        transpile_to_html5(self)
    }

    /// Transpiles this session into a standalone HTML5 document with a custom title.
    pub fn to_html5_with_title(&self, title: &str) -> String {
        let mut cloned = self.clone();
        cloned.title = title.to_string();
        transpile_to_html5(&cloned)
    }

    /// Transpiles this session into an EPUB 3.0 binary with default options.
    pub fn to_epub(&self) -> Result<Vec<u8>> {
        transpile_to_epub(self, None)
    }

    /// Transpiles this session into an EPUB 3.0 binary with custom options.
    pub fn to_epub_with_options(&self, options: &EpubOptions) -> Result<Vec<u8>> {
        crate::transpile::EpubTranspiler::transpile(self, options)
    }
}
