//! Error types for `alhalte-banquette`.

use thiserror::Error;

/// Result alias for operations in `alhalte-banquette`.
pub type Result<T> = std::result::Result<T, BanquetteError>;

/// Errors that can occur during room, scene, log, or state operations.
#[derive(Debug, Error)]
pub enum BanquetteError {
    /// State value exceeds the maximum allowable size of 4096 bytes (4KiB).
    #[error("State value size {size} bytes exceeds maximum allowed limit of {max} bytes")]
    StateValueTooLarge {
        /// Actual size in bytes.
        size: usize,
        /// Maximum limit (4096).
        max: usize,
    },

    /// State key is invalid (must contain at least one `::`).
    #[error("Invalid state key '{0}': key must contain at least one namespace separator '::'")]
    InvalidStateKey(String),

    /// Attempt to modify a reserved key in `std::*` without admin privileges.
    #[error(
        "Permission denied: key '{0}' is in the reserved namespace 'std::*' and requires room administrator privileges"
    )]
    ReservedKeyPermissionDenied(String),

    /// Permission denied for operation.
    #[error("Permission denied")]
    PermissionDenied,

    /// The operation would leave the room without a single administrator.
    #[error("Permission denied: a room must keep at least one administrator")]
    LastAdminDemotion,

    /// Persisted (or about to be persisted) room data violates a room invariant.
    ///
    /// This is also how a `RoomData` written by an incompatible schema is
    /// rejected: the payload either fails to decode or fails these checks, and
    /// the caller must refuse to start instead of opening an empty room.
    #[error("Invalid room data: {0}")]
    InvalidRoomData(String),

    /// Scene not found.
    #[error("Scene '{0}' not found")]
    SceneNotFound(String),

    /// Character not found.
    #[error("Character with ID {0} not found")]
    CharacterNotFound(u64),

    /// A persisted room tin belongs to a different room than the one being started.
    #[error("Persisted room tin belongs to room {actual}, expected room {expected}")]
    RoomIdMismatch {
        /// Room ID the process was started for.
        expected: u64,
        /// Room ID encoded in the persisted `RoomData`.
        actual: u64,
    },

    /// A persisted room tin's content hash does not match the head pointer.
    #[error("Persisted room tin id mismatch: expected {expected}, decoded content id {actual}")]
    TinIdMismatch {
        /// Tin ID stored as the room head pointer.
        expected: String,
        /// Tin ID computed from the downloaded bytes.
        actual: String,
    },

    /// Persisted room data does not contain the root scene `"/"`.
    #[error("Persisted room data is missing the root scene '/'")]
    MissingRootScene,

    /// Invalid log position requested for edit, delete, or rewind.
    #[error(
        "Invalid log position chunk {chunk}, pos {position}; current head is at chunk {head_chunk}, pos {head_position}"
    )]
    InvalidLogPosition {
        /// Requested chunk index.
        chunk: u32,
        /// Requested position in chunk.
        position: u32,
        /// Current head chunk index.
        head_chunk: u32,
        /// Current head position in chunk.
        head_position: u32,
    },

    /// Chunk capacity reached unexpectedly.
    #[error("Chunk is full")]
    ChunkFull,

    /// Error interacting with cellar storage service.
    #[error("Cellar error: {0}")]
    Cellar(String),

    /// Error from tin codec or container.
    #[error("Tin error: {0}")]
    Tin(#[from] tin::TinError),

    /// Postcard serialization or deserialization error.
    #[error("Postcard serialization error: {0}")]
    Postcard(#[from] postcard::Error),

    /// Standard I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A room export archive could not be built or read.
    #[error("Archive error: {0}")]
    Archive(String),

    /// General serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Protocol framing or socket communication error.
    #[error("Protocol error: {0}")]
    Protocol(String),
}
