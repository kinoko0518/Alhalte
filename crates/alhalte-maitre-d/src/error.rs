//! Error types for `alhalte-maitre-d`.

/// Result type alias for `alhalte-maitre-d` operations.
pub type Result<T> = std::result::Result<T, MaitreDError>;

/// Core error enumeration for `alhalte-maitre-d`.
#[derive(Debug, thiserror::Error)]
pub enum MaitreDError {
    /// SQLite database error.
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// I/O error during file or socket operation.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Google OAuth error.
    #[error("OAuth error: {0}")]
    OAuth(String),

    /// The configured authentication provider does not serve this flow.
    ///
    /// `maitre-d --local-login` installs the local provider, which has no Google
    /// code exchange or ID token verification. The HTTP layer maps this to
    /// `401 Unauthorized`; the Google routes are not even registered while the
    /// local login is active.
    #[error("Authentication flow is not available: {0}")]
    AuthFlowUnavailable(&'static str),

    /// Cryptographic or encryption error.
    #[error("Cryptographic error: {0}")]
    Crypto(String),

    /// Unauthorized access or invalid session.
    #[error("Unauthorized: invalid account ID or session ID")]
    Unauthorized,

    /// The local login account name or password did not match.
    ///
    /// Deliberately does not distinguish "no such account" from "wrong
    /// password": the local login API answers the same 401 for both.
    #[error("Unauthorized: invalid account name or password")]
    InvalidCredentials,

    /// An Argon2 password hash could not be created or parsed.
    ///
    /// The message is generic on purpose; neither the password nor the stored
    /// hash may appear in an error or a log line.
    #[error("password hashing error: {0}")]
    PasswordHash(String),

    /// Account not found.
    #[error("Account not found: {0}")]
    AccountNotFound(u64),

    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(u64),

    /// Banquette room process communication error.
    #[error("Banquette communication error: {0}")]
    Banquette(String),

    /// Banquette refused a room permission change.
    ///
    /// The room process answers a rejected `SetMemberPermission` /
    /// `SetDefaultPermission` with an `Error` frame (caller is not an
    /// administrator, last administrator demotion, ...). The HTTP layer maps
    /// this to `403 Forbidden`; it is distinct from a transport failure.
    #[error("Room permission refused: {0}")]
    RoomPermissionRefused(String),

    /// Serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid Tin container format or payload mismatch.
    #[error("Invalid tin format: {0}")]
    InvalidTin(String),

    /// Invalid room role.
    #[error("Invalid room role: {0}")]
    InvalidRole(String),

    /// A room with this ID is already registered.
    #[error("Room already exists: {0}")]
    RoomAlreadyExists(String),

    /// The database still holds rows of the retired `room_permissions` table.
    ///
    /// Issue #200 moved room authorization to `alhalte-banquette`'s `RoomData`
    /// and deliberately provides no migration: the operator must reset the
    /// database (delete the SQLite file) and let a fresh one be created.
    #[error(
        "database still holds {0} row(s) of the retired room_permissions table; room permissions are owned by alhalte-banquette now. Delete the SQLite database and restart"
    )]
    LegacyRoomPermissions(usize),

    /// The database still uses the retired `accounts.encrypted_sub` column.
    ///
    /// Issue #190 replaced the reversible ciphertext with a one-way 128-bit
    /// hash ([`crate::crypto::SubHash`]). There is no migration: existing
    /// accounts were never supported, so the operator must reset the database
    /// (delete the SQLite file) and let a fresh one be created.
    #[error(
        "database still uses the retired accounts.encrypted_sub column; Google sub identifiers are stored as a salted 128-bit hash now. Delete the SQLite database and restart"
    )]
    LegacySubSchema,

    /// A stored `sub` hash is not the expected 32-character hex form.
    ///
    /// The message never includes the offending value: it is a pseudonymous
    /// account correlator and must not reach logs.
    #[error("stored sub hash is malformed: expected 32 lowercase hex characters")]
    InvalidSubHash,

    /// The database still holds rooms with the retired `rooms.done` flag set.
    ///
    /// Like [`MaitreDError::LegacyRoomPermissions`], there is no migration: the
    /// public/private switch is the room's `default_permission` in banquette.
    #[error(
        "database still holds {0} room(s) with the retired rooms.done flag set; the public/private switch is alhalte-banquette's default_permission now. Delete the SQLite database and restart"
    )]
    LegacyRoomDone(usize),

    /// The database still holds rows of the retired `room_heads` table.
    ///
    /// Issue #192 moved the `room_id -> RoomData tin` pointer to cellar's
    /// generic alias `room/<room_id>/head`, which banquette reads on startup.
    /// There is no migration of old rows, so the operator must reset the
    /// database (delete the SQLite file) and let a fresh one be created.
    #[error(
        "database still holds {0} row(s) of the retired room_heads table; the room head is the cellar alias room/<room_id>/head now. Delete the SQLite database and restart"
    )]
    LegacyRoomHeads(usize),

    /// Invalid request parameters.
    #[error("Bad request: {0}")]
    BadRequest(String),
}
