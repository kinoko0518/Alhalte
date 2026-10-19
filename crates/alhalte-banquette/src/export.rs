//! Room export archives.
//!
//! `maitre-d` serves `GET /api/room/{room_id}/export/zip?type=full|logs` and
//! `banquette` builds the archive: the room itself is the authority on which
//! scenes and chunks a caller may export, so the ZIP is assembled next to the
//! snapshot instead of in the HTTP gateway (Issue #193).
//!
//! An archive is a ZIP with one `room.tin` entry (the exported [`RoomData`]) and
//! one `chunks/<tin-id>.tin` entry per exported [`LogChunk`]. Entries are
//! plaintext Tins (postcard payload, Zstd compressed). A `banquette` process
//! never holds the room key, so the archive is not room-key encrypted.

use std::io::{Cursor, Read};

use tin::{Compression, Tin, TinId};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{BanquetteError, Result};
use crate::labels::{LABEL_ALHALTE_BANQUETTE, LABEL_ALHALTE_BANQUETTE_LOG_CHUNK};
use crate::log::LogChunk;
use crate::room::RoomData;

/// Archive entry name of the exported [`RoomData`] tin.
pub const ROOM_ENTRY: &str = "room.tin";

/// Archive directory prefix of the exported [`LogChunk`] tins.
pub const CHUNK_ENTRY_DIR: &str = "chunks/";

/// An export archive unpacked back into the room snapshot and its log chunks.
///
/// This is the reader half of the archive format written by
/// [`export_room_to_zip`]. The snapshot is the same [`RoomData`] tin that a room
/// persists to cellar, so callers see the room exactly as banquette exported it.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomArchive {
    /// Room snapshot decoded from [`ROOM_ENTRY`].
    pub room: RoomData,
    /// Log chunks decoded from `chunks/*.tin`, with their tin id.
    pub chunks: Vec<(TinId, LogChunk)>,
}

impl RoomArchive {
    /// Loads an export ZIP from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns [`BanquetteError::Archive`] when `bytes` is not a ZIP, an entry is
    /// not a plaintext Tin, or the archive carries no [`RoomData`].
    pub fn load_from_slice(bytes: &[u8]) -> Result<Self> {
        if !is_zip(bytes) {
            return Err(BanquetteError::Archive(
                "unsupported archive format: ZIP header not detected".to_string(),
            ));
        }

        let mut zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| BanquetteError::Archive(format!("cannot open ZIP archive: {e}")))?;
        let mut room = None;
        let mut chunks = Vec::new();

        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|e| BanquetteError::Archive(format!("entry {index}: {e}")))?;
            if !entry.is_file() {
                continue;
            }
            let name = entry.name().to_string();
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data)?;

            let (tin, _, tin_id) = Tin::decode(&data, None)
                .map_err(|e| BanquetteError::Archive(format!("entry {name} is not a tin: {e}")))?;
            if tin.label == LABEL_ALHALTE_BANQUETTE {
                room = Some(postcard::from_bytes::<RoomData>(&tin.payload)?);
            } else if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_CHUNK {
                chunks.push((tin_id, postcard::from_bytes::<LogChunk>(&tin.payload)?));
            }
        }

        let room = room.ok_or_else(|| {
            BanquetteError::Archive(format!("archive carries no {ROOM_ENTRY} entry"))
        })?;
        Ok(Self { room, chunks })
    }
}

/// Returns `true` when `bytes` starts with a ZIP local-file or end-of-central-directory header.
fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

/// Maps a `zip` crate error into [`BanquetteError::Archive`].
fn archive_error(error: zip::result::ZipError) -> BanquetteError {
    BanquetteError::Archive(format!("ZIP error: {error}"))
}

/// Encodes `tin` as a plaintext (unencrypted) archive entry.
fn encode_entry(tin: &Tin) -> Result<Vec<u8>> {
    let (wire, _) = tin.encode_plaintext(Compression::Zstd)?;
    Ok(wire)
}

/// Builds the export ZIP for `room` and its log `chunks`.
///
/// `include_state == false` is the `type=logs` export: every scene keeps its
/// identity, biblio, and present characters, but all scene state is emptied
/// before the archive is written. The room-level tables (characters,
/// permissions) are always kept, and `room` itself is not mutated.
///
/// # Errors
///
/// Returns [`BanquetteError::Tin`] / [`BanquetteError::Archive`] when an entry
/// cannot be serialized or the ZIP cannot be written.
pub fn export_room_to_zip(
    room: &RoomData,
    chunks: &[(TinId, LogChunk)],
    include_state: bool,
) -> Result<Vec<u8>> {
    let mut room = room.clone();
    if !include_state {
        for scene in room.scenes.values_mut() {
            scene.states.clear();
        }
    }

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let room_tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &room)?;
    let room_wire = encode_entry(&room_tin)?;
    zip.start_file(ROOM_ENTRY, options).map_err(archive_error)?;
    std::io::copy(&mut Cursor::new(room_wire), &mut zip)?;

    for (tin_id, chunk) in chunks {
        let chunk_tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, chunk)?;
        let chunk_wire = encode_entry(&chunk_tin)?;
        let entry = format!("{CHUNK_ENTRY_DIR}{}.tin", hex::encode(tin_id));
        zip.start_file(entry, options).map_err(archive_error)?;
        std::io::copy(&mut Cursor::new(chunk_wire), &mut zip)?;
    }

    let cursor = zip.finish().map_err(archive_error)?;
    Ok(cursor.into_inner())
}
