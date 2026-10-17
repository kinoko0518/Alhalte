//! # alhalte-kitchen
//!
//! Room log archive processor, internal graph builder, and EPUB 3.0 / HTML5 transpiler for the Alhalte ecosystem.
//!
//! ## Overview
//!
//! In Alhalte, session logs from `alhalte-banquette` rooms can be archived as `.zip` or `.tar.gz` files.
//! `alhalte-kitchen` is responsible for:
//! - **Archive Loading**: Reading `.zip` and `.tar.gz` packages containing room data (`RoomData`),
//!   individual scene containers (`SceneData`), character repositories (`CharacterBiblio`), and sealed
//!   log chunks (`LogChunk`), supporting optional room key decryption (`0010` / `Encryption::RoomKey`).
//! - **Internal Graph Representation**: Analyzing character entry and departure events
//!   (`alhalte/banquette/log/character-came` and `alhalte/banquette/log/character-gone`) as well as
//!   chronological speech events to build [`RoomGraph`] with scene stay segments as nodes and character
//!   movements as directed edges.
//! - **Query Serialization**: Serializing session logs in chronological order via two primary querying modes:
//!   1. Tracing a specific character's path from a `start_node` to an `end_node`.
//!   2. Serializing an ordered list of scene IDs.
//! - **Transpilation**:
//!   - **HTML5**: Standalone HTML5 document with Material 3 Expressive styling (`alhalte-saignant`).
//!   - **EPUB 3.0**: Fully valid EPUB 3.0 binary verified against `epubcheck` with zero errors.
//!
//! ## Example
//!
//! ```
//! use alhalte_kitchen::{SerializedScene, SerializedSession};
//!
//! let mut session = SerializedSession::new("Session Title");
//! session.add_scene(SerializedScene {
//!     scene_id: "prologue".to_string(),
//!     title: "Prologue".to_string(),
//!     logs: Vec::new(),
//! });
//!
//! let html = session.to_html5();
//! assert!(html.contains("<!DOCTYPE html>"));
//! assert!(html.contains("Prologue"));
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod archive;
pub mod cli;
pub mod error;
pub mod graph;
pub mod serialize;
pub mod transpile;

pub use archive::{
    ArchiveScene, ParsedSceneEvent, RoomArchive, SceneEvent, export_room_to_tar_gz,
    export_room_to_zip,
};
pub use cli::{CliArgs, OutputFormat, parse_hex_room_key, run};
pub use error::{KitchenError, Result};
pub use graph::{GraphEdge, GraphNode, NodeId, RoomGraph};
pub use serialize::{SerializedScene, SerializedSession};
pub use transpile::{EpubOptions, EpubTranspiler, transpile_to_epub, transpile_to_html5};
