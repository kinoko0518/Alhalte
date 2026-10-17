//! Command-line interface definitions and execution logic.

use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, ValueEnum};

use crate::archive::RoomArchive;
use crate::error::{KitchenError, Result};
use crate::graph::RoomGraph;
use crate::transpile::EpubOptions;

/// Target output format for transpilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Complete standalone HTML5 document.
    Html5,
    /// EPUB 3.0 document package.
    Epub,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "html" | "html5" => Ok(Self::Html5),
            "epub" | "epub3" => Ok(Self::Epub),
            other => Err(format!("Unknown format '{other}', expected html5 or epub")),
        }
    }
}

/// Command-line arguments for `alhalte-kitchen`.
#[derive(Parser, Debug)]
#[command(
    name = "alhalte-kitchen",
    author = "Alhalte Authors",
    version = "0.1.0",
    about = "Room log archive processor, graph builder, and EPUB/HTML5 transpiler for Alhalte"
)]
pub struct CliArgs {
    /// Path to room archive file (`.zip` or `.tar.gz`).
    #[arg(value_name = "ARCHIVE")]
    pub archive: PathBuf,

    /// Target output format (`html5` or `epub`).
    #[arg(short, long, value_enum, default_value = "html5")]
    pub format: OutputFormat,

    /// Path to output file (defaults to stdout for HTML5, or `output.epub` for EPUB).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Character ID to trace path query for.
    #[arg(short = 'c', long)]
    pub character: Option<u64>,

    /// Start node or scene identifier for character path query.
    #[arg(long)]
    pub from: Option<String>,

    /// End node or scene identifier for character path query.
    #[arg(long)]
    pub to: Option<String>,

    /// Comma-separated list of scene IDs to serialize in specified order.
    #[arg(short = 's', long, value_delimiter = ',')]
    pub scenes: Vec<String>,

    /// Optional 32-byte hexadecimal room key for decryption (`Encryption::RoomKey`).
    #[arg(long)]
    pub room_key: Option<String>,

    /// Document title override.
    #[arg(short, long)]
    pub title: Option<String>,
}

/// Parses a 32-byte hexadecimal string into a byte array.
pub fn parse_hex_room_key(hex_str: &str) -> Result<[u8; 32]> {
    let raw = hex::decode(hex_str)
        .map_err(|e| KitchenError::InvalidArchive(format!("Invalid hexadecimal room key: {e}")))?;

    if raw.len() != 32 {
        return Err(KitchenError::InvalidArchive(format!(
            "Room key must be exactly 32 bytes (64 hex characters), got {} bytes",
            raw.len()
        )));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&raw);
    Ok(key)
}

/// Executes the CLI command with provided arguments.
pub fn run(args: CliArgs) -> Result<()> {
    let room_key = match args.room_key {
        Some(ref s) => Some(parse_hex_room_key(s)?),
        None => None,
    };

    let archive = RoomArchive::load_from_file(&args.archive, room_key)?;
    let graph = RoomGraph::build(&archive)?;

    // Determine query serialization
    let mut session =
        if let (Some(cid), Some(from), Some(to)) = (args.character, &args.from, &args.to) {
            graph.serialize_by_character_path(cid, from.as_str(), to.as_str())?
        } else if !args.scenes.is_empty() {
            graph.serialize_by_scene_ids(&args.scenes)?
        } else {
            // Fallback: serialize all scenes in graph in order
            let scene_ids: Vec<String> = archive.scenes.keys().cloned().collect();
            graph.serialize_by_scene_ids(&scene_ids)?
        };

    if let Some(ref title) = args.title {
        session.title = title.clone();
    }

    match args.format {
        OutputFormat::Html5 => {
            let html = session.to_html5();
            if let Some(out_path) = args.output {
                std::fs::write(out_path, html)?;
            } else {
                print!("{html}");
            }
        }
        OutputFormat::Epub => {
            let mut opts = EpubOptions::default();
            if !session.title.is_empty() {
                opts.title = session.title.clone();
            }
            let epub_bytes = session.to_epub_with_options(&opts)?;
            let out_path = args.output.unwrap_or_else(|| PathBuf::from("output.epub"));
            std::fs::write(out_path, epub_bytes)?;
        }
    }

    Ok(())
}
