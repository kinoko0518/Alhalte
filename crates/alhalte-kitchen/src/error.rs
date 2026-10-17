//! Error types for alhalte-kitchen.

use thiserror::Error;

use crate::graph::NodeId;

/// Result type alias for alhalte-kitchen operations.
pub type Result<T> = std::result::Result<T, KitchenError>;

/// Errors produced during archive loading, graph analysis, query serialization, or transpilation.
#[derive(Debug, Error)]
pub enum KitchenError {
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP processing error.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// Tin container error.
    #[error("Tin error: {0}")]
    Tin(#[from] tin::TinError),

    /// Banquette room protocol error.
    #[error("Banquette error: {0}")]
    Banquette(#[from] alhalte_banquette::BanquetteError),

    /// Saignant renderer error.
    #[error("Saignant error: {0}")]
    Saignant(#[from] alhalte_saignant::SaignantError),

    /// Postcard deserialization error.
    #[error("Postcard error: {0}")]
    Postcard(#[from] postcard::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Archive format invalid or missing required room/scene data.
    #[error("Invalid archive: {0}")]
    InvalidArchive(String),

    /// Requested scene was not found in room archive.
    #[error("Scene not found: {0}")]
    SceneNotFound(String),

    /// Requested character was not found.
    #[error("Character not found: {0}")]
    CharacterNotFound(u64),

    /// Requested graph node was not found.
    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),

    /// Directed path between start node and end node was not found for character.
    #[error("Path not found for character {character_id} from {start} to {end}")]
    PathNotFound {
        /// Character ID being traced.
        character_id: u64,
        /// Start node identifier.
        start: NodeId,
        /// End node identifier.
        end: NodeId,
    },

    /// Transpilation error.
    #[error("Transpile error: {0}")]
    Transpile(String),
}
