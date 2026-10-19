//! # alhalte-banquette
//!
//! Room daemon process for the Alhalte ecosystem managing scenes, logs, state relay, and WebRTC signaling.
//!
//! ## Overview
//!
//! In Alhalte, sessions take place in rooms hosted by `alhalte-banquette` processes.
//! A room maintains:
//! - **Scene biblio**: Mapping from `scene-id` (`String`) to [`Scene`], where `"/"` is the root virtual scene.
//! - **Character biblio**: Mapping from `character-id` (`u64`) to [`Character`] (`HashMap<String, Tin>`).
//!   Character IDs are generated monotonically without collisions.
//! - **Scene logs**: Chunked log history of `1 << N` entries per chunk (default $N = 8$, i.e. 256 entries),
//!   supporting `append`, `edit`, `delete` (tombstone), and `rewind`. When chunks fill up, they are sealed into
//!   `alhalte/banquette/log/chunk` tins and uploaded to `alhalte-cellar`.
//! - **Scene state**: High-frequency associative state (`BTreeMap<String, Tin>`) bounded to 4KiB per tin value,
//!   separated by the `::` key namespace.
//! - **Access control**: Every account has one [`RoomPermissionLevel`]; `Denied` and `Viewer` accounts
//!   receive **no response** to queries for a scene log, state, or name they may not read, and every
//!   mutate frame is refused (`Viewer` never writes, `Participant` writes only the scenes an
//!   administrator moved it into plus `"/"`, `Admin` writes everything).
//! - **Lifecycle**: Unloading idle scenes when active client counts drop to zero, and periodic 4-minute heartbeats
//!   with room state persistence, shutting down when idle for 3 consecutive intervals.
//! - **Persistence**: [`Room::persist`] stores the whole room in cellar and reports the tin ID so
//!   [`Room::load_from_cellar`] can restore it on the next process start. Internal writes are authorized
//!   by the cellar service token carried by the [`CellarSink`], never by a synthetic user session.
//!
//! ## Example
//!
//! ```
//! use alhalte_banquette::Room;
//!
//! let mut room = Room::new(1001, 42, 8).unwrap();
//! assert_eq!(room.room_id, 1001);
//! assert!(room.is_admin(42));
//!
//! room.create_scene(42, "tavern", None).unwrap();
//! assert!(room.list_scene_ids().contains(&"tavern".to_string()));
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cellar_sink;
pub mod character;
pub mod cli;
pub mod error;
pub mod labels;
pub mod log;
pub mod protocol;
pub mod room;
pub mod scene;
pub mod server;
pub mod state;

pub use cellar_sink::{CellarSink, HttpCellarSink, INTERNAL_WRITE_SESSION_ID, InMemoryCellarSink};
pub use character::{
    Character, CharacterBiblio, get_character_name, get_character_owner, set_character_name,
    set_character_owner,
};
pub use cli::{CliArgs, parse_cli_args};
pub use error::{BanquetteError, Result};
pub use labels::{
    IDENT_ALHALTE_BANQUETTE, IDENT_ALHALTE_BANQUETTE_LOG_BIBLIO,
    IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
    IDENT_ALHALTE_BANQUETTE_LOG_CHUNK, IDENT_ALHALTE_BANQUETTE_SCENE,
    IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID, IDENT_ALHALTE_MAITRE_D_SESSION_ID, KEY_STD_NAME,
    KEY_STD_OWNER_ACCOUNT, LABEL_ALHALTE_BANQUETTE, LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO,
    LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
    LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, LABEL_ALHALTE_BANQUETTE_SCENE,
    LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, LABEL_ALHALTE_MAITRE_D_SESSION_ID,
};
pub use log::{
    CharacterCameEvent, CharacterGoneEvent, LogBiblio, LogChunk, LogOperationEvent, SceneLog,
};
pub use protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
    WebRtcSignalPayload,
};
pub use room::{
    HeartbeatOutcome, HeartbeatResult, Room, RoomData, RoomPermissionLevel, SceneSummary,
};
pub use scene::{Scene, SceneData};
pub use server::{BanquetteServer, ConnectedClient, ServerConfig};
pub use state::{
    MAX_STATE_VALUE_SIZE, SceneState, is_reserved_key, validate_state_key, validate_state_value,
};
