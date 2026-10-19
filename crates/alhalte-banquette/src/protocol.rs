//! Protocol frames and framing codec for UNIX domain socket communication with `alhalte-maitre-d`.

use std::collections::{BTreeMap, HashMap};

use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{Decoder, Encoder};

use tin::Tin;

use crate::character::Character;
use crate::error::{BanquetteError, Result};
use crate::log::LogBiblio;
use crate::room::{RoomPermissionLevel, SceneSummary};

/// WebRTC signaling payloads exchanged between clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebRtcSignalPayload {
    /// SDP Offer description.
    Offer(String),
    /// SDP Answer description.
    Answer(String),
    /// ICE Candidate string.
    IceCandidate(String),
}

/// Commands and requests initiated by a client WebSocket connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientFrame {
    /// List all scene IDs in the room (public information).
    ListScenes,
    /// Query scene details. Returns nothing if unpermitted.
    QueryScene {
        /// Scene ID to query.
        scene_id: String,
    },
    /// Create a new scene.
    CreateScene {
        /// New scene ID.
        scene_id: String,
        /// Whether to inherit juices (BGM/background image) from root.
        inherit_juices: bool,
        /// Optional human-readable scene name written to `std::name` at creation.
        ///
        /// `None` keeps the scene name empty so `SceneSummary` falls back to the
        /// scene ID. Declared after the pre-existing fields so the postcard
        /// struct layout of the earlier fields stays stable.
        name: Option<String>,
    },
    /// Delete a scene (admin only).
    DeleteScene {
        /// Scene ID to delete.
        scene_id: String,
    },
    /// Client enters a scene.
    EnterScene {
        /// Scene ID.
        scene_id: String,
    },
    /// Client leaves a scene.
    LeaveScene {
        /// Scene ID.
        scene_id: String,
    },
    /// Query all state in a scene. Returns nothing if unpermitted.
    QueryState {
        /// Target scene ID.
        scene_id: String,
    },
    /// Update or insert a key-value pair in scene state.
    UpdateState {
        /// Target scene ID.
        scene_id: String,
        /// State key (must contain `::`, max size 4KiB).
        key: String,
        /// Value Tin container.
        value: Tin,
    },
    /// Delete a key from scene state.
    DeleteState {
        /// Target scene ID.
        scene_id: String,
        /// State key.
        key: String,
    },
    /// Append a log entry to a scene.
    AppendLog {
        /// Target scene ID.
        scene_id: String,
        /// Log entry (or tombstone).
        entry: Option<Tin>,
    },
    /// Edit an existing entry before the head.
    EditLog {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
        /// New entry.
        entry: Tin,
    },
    /// Delete an existing entry before the head (tombstone).
    DeleteLog {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
    },
    /// Rewind head pointer to an earlier position.
    RewindLog {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
    },
    /// Query scene log biblio. Returns nothing if unpermitted.
    QueryLogBiblio {
        /// Target scene ID.
        scene_id: String,
    },
    /// Query a chunk of log entries. Returns nothing if unpermitted.
    QueryLogChunk {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index to fetch.
        chunk_index: u32,
    },
    /// List all characters in the room.
    ListCharacters,
    /// Query a single character.
    QueryCharacter {
        /// Character ID.
        character_id: u64,
    },
    /// Create a new character.
    CreateCharacter {
        /// Initial character attributes.
        data: Character,
    },
    /// Update an existing character.
    UpdateCharacter {
        /// Character ID.
        character_id: u64,
        /// New character attributes.
        data: Character,
    },
    /// Delete a character.
    DeleteCharacter {
        /// Character ID.
        character_id: u64,
    },
    /// Move a character to a scene.
    MoveCharacter {
        /// Character ID.
        character_id: u64,
        /// Destination scene ID.
        to_scene_id: String,
    },
    /// Relay WebRTC signaling message to a peer client in the same scene.
    WebRtcSignal {
        /// Destination client ID.
        to_client_id: u64,
        /// Signaling payload.
        payload: WebRtcSignalPayload,
    },
    /// Response confirming client liveness to a heartbeat ping.
    HeartbeatResponse,
    /// Sets the permission of a single account (admin only).
    ///
    /// Emits [`ServerFrame::PermissionsUpdated`] to the requester on success and
    /// [`ServerFrame::Error`] with `code = "SetMemberPermissionFailed"`
    /// otherwise. Demoting the last administrator is rejected.
    ///
    /// Declared after the pre-existing variants; Issue #199 intentionally broke
    /// wire compatibility by removing `SetDone`, so the indices of the frames
    /// after it shifted by one.
    SetMemberPermission {
        /// Target account ID.
        account_id: u64,
        /// New permission level for the account.
        permission: RoomPermissionLevel,
    },
    /// Sets the permission used for accounts without an explicit entry (admin only).
    ///
    /// Emits [`ServerFrame::PermissionsUpdated`] to the requester on success and
    /// [`ServerFrame::Error`] with `code = "SetDefaultPermissionFailed"`
    /// otherwise.
    SetDefaultPermission {
        /// New default permission level.
        permission: RoomPermissionLevel,
    },
    /// Requests the current permission table.
    ///
    /// Answered with [`ServerFrame::PermissionsSnapshot`]; an account without
    /// access to the room gets no response, like the other query frames.
    QueryPermissions,
}

/// Messages emitted from `banquette` to a client WebSocket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerFrame {
    /// List of scene summaries.
    ///
    /// Carries the human-readable `std::name` alongside the public identifiers
    /// so a client can render the room without a separate `QueryScene` per
    /// scene.
    SceneList {
        /// Public scene summaries, sorted by scene ID.
        scenes: Vec<SceneSummary>,
    },
    /// Scene summary information.
    SceneInfo(SceneSummary),
    /// Snapshot of scene state.
    StateSnapshot {
        /// Target scene ID.
        scene_id: String,
        /// States map.
        states: BTreeMap<String, Tin>,
    },
    /// Notification of updated state key.
    StateUpdated {
        /// Target scene ID.
        scene_id: String,
        /// State key.
        key: String,
        /// New value.
        value: Tin,
    },
    /// Notification of deleted state key.
    StateDeleted {
        /// Target scene ID.
        scene_id: String,
        /// State key.
        key: String,
    },
    /// Snapshot of scene log biblio.
    LogBiblioSnapshot {
        /// Target scene ID.
        scene_id: String,
        /// Biblio.
        biblio: LogBiblio,
    },
    /// Snapshot of a log chunk.
    LogChunkSnapshot {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Chunk entries.
        entries: Vec<Option<Tin>>,
    },
    /// Notification that a new entry was appended.
    LogEventAppended {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
        /// Entry value.
        entry: Option<Tin>,
    },
    /// Notification that an entry was edited.
    LogEventEdited {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
        /// New entry value.
        entry: Tin,
    },
    /// Notification that an entry was deleted.
    LogEventDeleted {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
    },
    /// Notification that head was rewound.
    LogEventRewound {
        /// Target scene ID.
        scene_id: String,
        /// Chunk index.
        chunk_index: u32,
        /// Position in chunk.
        position: u32,
    },
    /// List of all characters.
    CharacterList {
        /// Map of character ID to character attributes.
        characters: HashMap<u64, Character>,
    },
    /// Single character info.
    CharacterInfo {
        /// Character ID.
        character_id: u64,
        /// Character attributes.
        data: Character,
    },
    /// Confirmation of character creation.
    CharacterCreated {
        /// Allocated character ID.
        character_id: u64,
    },
    /// Confirmation of character update.
    CharacterUpdated {
        /// Updated character ID.
        character_id: u64,
    },
    /// Confirmation of character deletion.
    CharacterDeleted {
        /// Deleted character ID.
        character_id: u64,
    },
    /// Relayed WebRTC signaling message from a peer.
    WebRtcSignal {
        /// Sender client ID.
        from_client_id: u64,
        /// Signaling payload.
        payload: WebRtcSignalPayload,
    },
    /// Heartbeat ping from server.
    HeartbeatPing,
    /// Explicit error report.
    Error {
        /// Machine-readable code.
        code: String,
        /// Explanation message.
        message: String,
    },
    /// Confirmation that a scene was created.
    ///
    /// Declared after [`ServerFrame::Error`] so that the postcard variant indices of
    /// pre-existing frames stay stable (backward compatibility).
    SceneCreated {
        /// Created scene ID.
        scene_id: String,
    },
    /// Confirmation that a scene was deleted.
    SceneDeleted {
        /// Deleted scene ID.
        scene_id: String,
    },
    /// Confirmation that a character was moved to another scene.
    CharacterMoved {
        /// Moved character ID.
        character_id: u64,
        /// Destination scene ID.
        to_scene_id: String,
    },
    /// Complete permission table of the room.
    ///
    /// The answer to [`ClientFrame::QueryPermissions`]. Also the success
    /// acknowledgement of [`ClientFrame::SetMemberPermission`] /
    /// [`ClientFrame::SetDefaultPermission`] is
    /// [`ServerFrame::PermissionsUpdated`], never a snapshot.
    ///
    /// Declared after the pre-existing variants; Issue #199 removed
    /// `DoneUpdated`, so the earlier variant indices are unchanged.
    PermissionsSnapshot {
        /// Permission applied to accounts without an explicit entry.
        default_permission: RoomPermissionLevel,
        /// Per-account permission overrides.
        permissions: BTreeMap<u64, RoomPermissionLevel>,
    },
    /// The room permission table changed.
    ///
    /// Sent to the administrator whose request succeeded. A rejected request
    /// emits [`ServerFrame::Error`] with `SetMemberPermissionFailed` /
    /// `SetDefaultPermissionFailed` instead, and nothing changes.
    PermissionsUpdated {
        /// Permission applied to accounts without an explicit entry.
        default_permission: RoomPermissionLevel,
        /// Per-account permission overrides.
        permissions: BTreeMap<u64, RoomPermissionLevel>,
    },
}

/// Multiplexed messages sent from `maitre-d` gateway into `banquette` over UNIX domain socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaitreDToBanquette {
    /// A client connected to the room via WebSocket.
    ClientConnected {
        /// Assigned connection ID.
        client_id: u64,
        /// Authenticated account ID.
        account_id: u64,
        /// Validated session ID.
        session_id: u64,
    },
    /// A client disconnected.
    ClientDisconnected {
        /// Client ID.
        client_id: u64,
    },
    /// A multiplexed frame received from a specific client.
    ClientMessage {
        /// Sender client ID.
        client_id: u64,
        /// Client frame payload.
        frame: ClientFrame,
    },
    /// Keep-alive ping from `maitre-d`.
    Ping,
    /// Request an export archive of the room.
    ///
    /// `banquette` assembles the ZIP itself (Issue #193): it is the authority on
    /// the caller's readable scenes, so the archive is built from the same
    /// snapshot that enforces `Room::can_access_scene`.
    ExportRoom {
        /// Request correlation ID.
        request_id: u64,
        /// Calling account ID, if authenticated.
        caller_account: Option<u64>,
        /// Whether scene state is included (`type=full`) or emptied
        /// (`type=logs`).
        include_state: bool,
    },
    /// Asks the running room for the effective permission of one account.
    ///
    /// This is how `maitre-d` learns what `banquette` already considers the
    /// account to be (`RoomData.permissions` is the source of truth, not the
    /// `maitre-d` database).
    ///
    /// Declared last so that the postcard variant indices of pre-existing
    /// frames stay stable; `SetAdmins` was removed by Issue #199.
    QueryAccountPermission {
        /// Request correlation ID.
        request_id: u64,
        /// Account ID to look up.
        account_id: u64,
    },
}

/// Outgoing responses and broadcasts from `banquette` to `maitre-d`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BanquetteToMaitreD {
    /// Send frame to a specific client.
    SendToClient {
        /// Recipient client ID.
        client_id: u64,
        /// Server frame.
        frame: ServerFrame,
    },
    /// Broadcast frame to all active clients in a scene.
    BroadcastToScene {
        /// Scene ID.
        scene_id: String,
        /// Server frame.
        frame: ServerFrame,
    },
    /// Broadcast frame to all active clients in the room.
    BroadcastToRoom {
        /// Server frame.
        frame: ServerFrame,
    },
    /// Keep-alive pong response to `maitre-d`.
    Pong,
    /// Room process is terminating and shutting down.
    RoomTerminated {
        /// Reason for termination (e.g. idle timeout).
        reason: String,
    },
    /// Response carrying the export ZIP, or an error description.
    ExportRoomResponse {
        /// Request correlation ID.
        request_id: u64,
        /// ZIP archive bytes (see [`crate::export::export_room_to_zip`]), or an
        /// error description when the room could not be exported.
        result: std::result::Result<Vec<u8>, String>,
    },
    /// Answer to [`MaitreDToBanquette::QueryAccountPermission`].
    ///
    /// Declared last so that the postcard variant indices of pre-existing
    /// frames stay stable; `AdminsUpdated` was removed by Issue #199 and
    /// `RoomPersisted` by Issue #192.
    AccountPermissionResponse {
        /// Request correlation ID echoed from the query.
        request_id: u64,
        /// Account ID that was looked up.
        account_id: u64,
        /// Effective permission of the account in this room.
        permission: RoomPermissionLevel,
    },
}

/// Length-delimited Postcard codec for protocol stream communication.
#[derive(Debug, Default)]
pub struct ProtocolCodec<Item> {
    _marker: std::marker::PhantomData<Item>,
}

impl<Item> ProtocolCodec<Item> {
    /// Creates a new [`ProtocolCodec`].
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Item: serde::de::DeserializeOwned> Decoder for ProtocolCodec<Item> {
    type Item = Item;
    type Error = BanquetteError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[0..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        src.advance(4);
        let payload = src.split_to(length);
        let item: Item = postcard::from_bytes(&payload)?;
        Ok(Some(item))
    }
}

impl<Item: serde::Serialize> Encoder<Item> for ProtocolCodec<Item> {
    type Error = BanquetteError;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<()> {
        let bytes = postcard::to_allocvec(&item)?;
        let len = bytes.len() as u32;
        dst.reserve(4 + bytes.len());
        dst.put_u32(len);
        dst.put_slice(&bytes);
        Ok(())
    }
}
