//! Archive loader and exporter for room data packaged in `.zip` or `.tar.gz` formats.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use alhalte_banquette::character::Character;
use alhalte_banquette::labels::{
    LABEL_ALHALTE_BANQUETTE, LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO,
    LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG, LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BGM,
    LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
    LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, LABEL_ALHALTE_BANQUETTE_SCENE,
};
use alhalte_banquette::log::{
    ChangeBgImgEvent, ChangeBgmEvent, CharacterCameEvent, CharacterGoneEvent, LogChunk,
};
use alhalte_banquette::room::RoomData;
use alhalte_banquette::scene::SceneData;
use alhalte_saignant::LogEntry;
use flate2::Compression as GzCompression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use generast::LABEL_ALHALTE_GENERAST;
use tar::{Archive as TarArchive, Builder as TarBuilder};
use tin::{Compression, Tin, TinId};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{KitchenError, Result};

/// An event occurring within a scene log sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneEvent {
    /// A chat/speech entry or general log entry rendered by saignant.
    Speech(LogEntry),
    /// Character entered the scene.
    CharacterCame {
        /// Character identifier.
        character_id: u64,
    },
    /// Character left the scene.
    CharacterGone {
        /// Character identifier.
        character_id: u64,
    },
    /// Background image change event.
    ChangeBgImg(ChangeBgImgEvent),
    /// Background music change event.
    ChangeBgm(ChangeBgmEvent),
}

/// A parsed scene event with ordering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSceneEvent {
    /// Associated scene identifier.
    pub scene_id: String,
    /// Event timestamp in UNIX seconds.
    pub timestamp: u64,
    /// Sequential order within the scene logs.
    pub order: usize,
    /// The event payload.
    pub event: SceneEvent,
}

/// In-memory representation of a scene extracted from an archive.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArchiveScene {
    /// Scene identifier (e.g. `"/"`, `"tavern"`).
    pub id: String,
    /// Scene state key-value mapping.
    pub states: BTreeMap<String, Tin>,
    /// Characters present in this scene initially.
    pub present_characters: Vec<u64>,
    /// Chunks of log entries belonging to this scene.
    pub chunks: Vec<LogChunk>,
    /// Sealed chunk Tin IDs recorded in the scene's log biblio.
    pub biblio_chunks: Vec<TinId>,
}

impl ArchiveScene {
    /// Parses all events from all chunks in this scene into sequential [`ParsedSceneEvent`]s.
    pub fn parse_all_events(&self) -> Vec<ParsedSceneEvent> {
        let mut raw_events = Vec::new();
        let mut order = 0;

        for chunk in &self.chunks {
            for entry_opt in &chunk.entries {
                let Some(tin) = entry_opt else {
                    order += 1;
                    continue;
                };

                let (event, ts_opt) = parse_entry_tin(tin);
                raw_events.push((event, ts_opt, order));
                order += 1;
            }
        }

        // Two-pass timestamp interpolation:
        // Pass 1: Forward propagate known timestamps
        let mut last_known_ts = None;
        let mut fwd_ts = Vec::with_capacity(raw_events.len());
        for (_, ts_opt, _) in &raw_events {
            if let Some(ts) = ts_opt {
                last_known_ts = Some(*ts);
                fwd_ts.push(Some(*ts));
            } else {
                fwd_ts.push(last_known_ts);
            }
        }

        // Pass 2: Backward propagate for leading un-timestamped events
        let first_known_ts = fwd_ts.iter().flatten().copied().next().unwrap_or(0);

        let mut events = Vec::with_capacity(raw_events.len());
        let mut current_ts = first_known_ts;
        for (i, (event, _, order)) in raw_events.into_iter().enumerate() {
            if let Some(ts) = fwd_ts[i] {
                current_ts = ts;
            }
            events.push(ParsedSceneEvent {
                scene_id: self.id.clone(),
                timestamp: current_ts,
                order,
                event,
            });
        }

        events
    }
}

/// Extracts a [`SceneEvent`] and optional timestamp from a `Tin` entry.
fn parse_entry_tin(tin: &Tin) -> (SceneEvent, Option<u64>) {
    if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME {
        if let Ok(ev) = CharacterCameEvent::from_tin(tin) {
            return (
                SceneEvent::CharacterCame {
                    character_id: ev.character_id,
                },
                None,
            );
        }
    } else if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE
        && let Ok(ev) = CharacterGoneEvent::from_tin(tin)
    {
        return (
            SceneEvent::CharacterGone {
                character_id: ev.character_id,
            },
            None,
        );
    } else if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG
        && let Ok(ev) = ChangeBgImgEvent::from_tin(tin)
    {
        return (SceneEvent::ChangeBgImg(ev), None);
    } else if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BGM
        && let Ok(ev) = ChangeBgmEvent::from_tin(tin)
    {
        return (SceneEvent::ChangeBgm(ev), None);
    }

    // Try duck-typed HashMap<String, Tin>
    if let Ok(map) = postcard::from_bytes::<HashMap<String, Tin>>(&tin.payload) {
        let explicit_ts = map.get("timestamp").and_then(|t| t.as_unix_time().ok());

        // Check for duck-typed came/gone
        if let Some(t) = map.get("type").and_then(|t| t.as_string().ok()) {
            if (t == "character-came" || t == "came")
                && let Some(cid) = map.get("character_id").and_then(|t| t.as_u64().ok())
            {
                return (SceneEvent::CharacterCame { character_id: cid }, explicit_ts);
            } else if (t == "character-gone" || t == "gone")
                && let Some(cid) = map.get("character_id").and_then(|t| t.as_u64().ok())
            {
                return (SceneEvent::CharacterGone { character_id: cid }, explicit_ts);
            }
        }

        let log_entry = LogEntry::from_map(map);
        let ts = if log_entry.unix_time() > 0 {
            Some(log_entry.unix_time())
        } else {
            explicit_ts
        };
        return (SceneEvent::Speech(log_entry), ts);
    }

    // Direct Generast payload
    if tin.label == LABEL_ALHALTE_GENERAST
        && let Ok(ast) = generast::Generast::from_tin(tin)
    {
        let log_entry = LogEntry::new().with_content_ast(&ast);
        return (SceneEvent::Speech(log_entry), None);
    }

    // Primitive string payload
    if let Ok(s) = tin.as_string() {
        let ast = generast::Generast::string(s);
        let log_entry = LogEntry::new().with_content_ast(&ast);
        return (SceneEvent::Speech(log_entry), None);
    }

    // Fallback speech entry
    let log_entry = LogEntry::new();
    (SceneEvent::Speech(log_entry), None)
}

/// Unpacked room archive containing room metadata, character definitions, and scenes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoomArchive {
    /// Optional 64-bit room identifier.
    pub room_id: Option<u64>,
    /// Whether room is fully public (`done` flag).
    pub done: bool,
    /// Log chunk size exponent $N$.
    pub chunk_exponent: u8,
    /// Character repository mapping `character_id` to attribute map.
    pub characters: HashMap<u64, Character>,
    /// Scene mapping from `scene_id` to [`ArchiveScene`].
    pub scenes: HashMap<String, ArchiveScene>,
}

impl RoomArchive {
    /// Loads a room archive from raw bytes (auto-detecting `.zip` or `.tar.gz`).
    pub fn load_from_slice(bytes: &[u8], room_key: Option<[u8; 32]>) -> Result<Self> {
        if bytes.starts_with(b"PK\x03\x04")
            || bytes.starts_with(b"PK\x05\x06")
            || bytes.starts_with(b"PK\x07\x08")
        {
            Self::load_zip(Cursor::new(bytes), room_key)
        } else if bytes.starts_with(&[0x1f, 0x8b]) {
            Self::load_tar_gz(bytes, room_key)
        } else {
            Err(KitchenError::InvalidArchive(
                "Unsupported archive format: neither ZIP nor gzip header detected".to_string(),
            ))
        }
    }

    /// Loads a room archive from a file on disk.
    pub fn load_from_file(path: impl AsRef<Path>, room_key: Option<[u8; 32]>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::load_from_slice(&bytes, room_key)
    }

    /// Loads a room archive from a `.zip` stream.
    pub fn load_zip<R: Read + Seek>(reader: R, room_key: Option<[u8; 32]>) -> Result<Self> {
        let mut zip = ZipArchive::new(reader)?;
        let mut files = HashMap::new();

        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            if file.is_file() {
                let name = file.name().to_string();
                let mut data = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut data)?;
                files.insert(name, data);
            }
        }

        Self::parse_extracted_files(files, room_key)
    }

    /// Loads a room archive from a `.tar.gz` stream.
    pub fn load_tar_gz<R: Read>(reader: R, room_key: Option<[u8; 32]>) -> Result<Self> {
        let gz = GzDecoder::new(reader);
        let mut tar = TarArchive::new(gz);
        let mut files = HashMap::new();

        for entry_res in tar.entries()? {
            let mut entry = entry_res?;
            if entry.header().entry_type().is_file() {
                let path = entry.path()?.to_string_lossy().to_string();
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                files.insert(path, data);
            }
        }

        Self::parse_extracted_files(files, room_key)
    }

    /// Parses a set of unpacked files into a [`RoomArchive`].
    fn parse_extracted_files(
        files: HashMap<String, Vec<u8>>,
        room_key: Option<[u8; 32]>,
    ) -> Result<Self> {
        let mut archive = Self {
            chunk_exponent: 8,
            ..Default::default()
        };

        let mut chunk_map: HashMap<TinId, LogChunk> = HashMap::new();
        let mut path_chunks: HashMap<String, LogChunk> = HashMap::new();

        // Pass 1: Decode all files as Tins or parse JSON
        for (filename, bytes) in &files {
            // Attempt Tin decoding with optional room_key
            if let Ok((tin, _, tin_id)) = Tin::decode(bytes, room_key.as_ref()) {
                if tin.label == LABEL_ALHALTE_BANQUETTE {
                    if let Ok(room_data) = postcard::from_bytes::<RoomData>(&tin.payload) {
                        archive.room_id = Some(room_data.room_id);
                        archive.done = room_data.done;
                        archive.chunk_exponent = room_data.chunk_exponent;
                        archive.characters = room_data.character_biblio.all().clone();

                        for (sid, sdata) in room_data.scenes {
                            archive
                                .scenes
                                .entry(sid.clone())
                                .or_insert_with(|| ArchiveScene {
                                    id: sdata.id,
                                    states: sdata.states,
                                    present_characters: sdata.present_characters,
                                    chunks: Vec::new(),
                                    biblio_chunks: sdata.biblio.chunks,
                                });
                        }
                    }
                } else if tin.label == LABEL_ALHALTE_BANQUETTE_SCENE {
                    if let Ok(sdata) = postcard::from_bytes::<SceneData>(&tin.payload) {
                        archive
                            .scenes
                            .entry(sdata.id.clone())
                            .or_insert_with(|| ArchiveScene {
                                id: sdata.id,
                                states: sdata.states,
                                present_characters: sdata.present_characters,
                                chunks: Vec::new(),
                                biblio_chunks: sdata.biblio.chunks,
                            });
                    }
                } else if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_CHUNK {
                    if let Ok(chunk) = postcard::from_bytes::<LogChunk>(&tin.payload) {
                        chunk_map.insert(tin_id, chunk.clone());
                        path_chunks.insert(filename.clone(), chunk);
                    }
                } else if tin.label == LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO {
                    // Log biblio definition
                }
                continue;
            }

            // Fallback: Check if file is JSON
            if filename.ends_with(".json") {
                if let Ok(room_data) = serde_json::from_slice::<RoomData>(bytes) {
                    archive.room_id = Some(room_data.room_id);
                    archive.done = room_data.done;
                    archive.chunk_exponent = room_data.chunk_exponent;
                    archive.characters = room_data.character_biblio.all().clone();

                    for (sid, sdata) in room_data.scenes {
                        archive
                            .scenes
                            .entry(sid.clone())
                            .or_insert_with(|| ArchiveScene {
                                id: sdata.id,
                                states: sdata.states,
                                present_characters: sdata.present_characters,
                                chunks: Vec::new(),
                                biblio_chunks: sdata.biblio.chunks,
                            });
                    }
                } else if let Ok(sdata) = serde_json::from_slice::<SceneData>(bytes) {
                    archive
                        .scenes
                        .entry(sdata.id.clone())
                        .or_insert_with(|| ArchiveScene {
                            id: sdata.id,
                            states: sdata.states,
                            present_characters: sdata.present_characters,
                            chunks: Vec::new(),
                            biblio_chunks: sdata.biblio.chunks,
                        });
                } else if let Ok(chunk) = serde_json::from_slice::<LogChunk>(bytes) {
                    path_chunks.insert(filename.clone(), chunk);
                }
            }
        }

        // Pass 2: Associate log chunks with scenes
        for scene in archive.scenes.values_mut() {
            // First: match by biblio chunks
            for tid in &scene.biblio_chunks {
                if let Some(chunk) = chunk_map.get(tid) {
                    scene.chunks.push(chunk.clone());
                } else {
                    // Check if filename contains hex TinId
                    let hex_id = hex::encode(tid);
                    for (name, chunk) in &path_chunks {
                        if name.contains(&hex_id) {
                            scene.chunks.push(chunk.clone());
                            break;
                        }
                    }
                }
            }

            // Second: if no chunks populated from biblio, search by scene path
            if scene.chunks.is_empty() {
                let scene_prefix = format!("{}/", scene.id);
                let scene_prefix2 = format!("scenes/{}/", scene.id);
                let mut matched_chunks: Vec<(String, LogChunk)> = path_chunks
                    .iter()
                    .filter(|(name, _)| {
                        name.contains(&scene_prefix) || name.contains(&scene_prefix2)
                    })
                    .map(|(name, chunk)| (name.clone(), chunk.clone()))
                    .collect();

                matched_chunks.sort_by(|a, b| a.0.cmp(&b.0));
                for (_, chunk) in matched_chunks {
                    scene.chunks.push(chunk);
                }
            }
        }

        // If there's only 1 scene and unassociated chunks remain, assign them
        if archive.scenes.len() == 1 {
            let scene = archive.scenes.values_mut().next().unwrap();
            if scene.chunks.is_empty() && !path_chunks.is_empty() {
                let mut sorted_chunks: Vec<_> = path_chunks.into_values().collect();
                scene.chunks.append(&mut sorted_chunks);
            }
        }

        if archive.scenes.is_empty() {
            return Err(KitchenError::InvalidArchive(
                "No valid scenes found in archive".to_string(),
            ));
        }

        Ok(archive)
    }
}

/// Exports a room state and associated log chunks into a `.zip` archive byte buffer.
pub fn export_room_to_zip(
    room: &RoomData,
    chunks: &[(TinId, LogChunk)],
    room_key: Option<[u8; 32]>,
) -> Result<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // 1. Write room.tin
    let room_tin =
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, room).map_err(KitchenError::Tin)?;
    let (room_wire, _) = if let Some(key) = room_key {
        room_tin
            .encode_room_key(Compression::Zstd, &key, &[0u8; 12])
            .map_err(KitchenError::Tin)?
    } else {
        room_tin
            .encode_plaintext(Compression::Zstd)
            .map_err(KitchenError::Tin)?
    };

    zip.start_file("room.tin", options)?;
    std::io::copy(&mut Cursor::new(room_wire), &mut zip)?;

    // 2. Write chunks
    for (tid, chunk) in chunks {
        let chunk_tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, chunk)
            .map_err(KitchenError::Tin)?;
        let (chunk_wire, _) = if let Some(key) = room_key {
            chunk_tin
                .encode_room_key(Compression::Zstd, &key, &[0u8; 12])
                .map_err(KitchenError::Tin)?
        } else {
            chunk_tin
                .encode_plaintext(Compression::Zstd)
                .map_err(KitchenError::Tin)?
        };

        let fname = format!("chunks/{}.tin", hex::encode(tid));
        zip.start_file(fname, options)?;
        std::io::copy(&mut Cursor::new(chunk_wire), &mut zip)?;
    }

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

/// Exports a room state and associated log chunks into a `.tar.gz` archive byte buffer.
pub fn export_room_to_tar_gz(
    room: &RoomData,
    chunks: &[(TinId, LogChunk)],
    room_key: Option<[u8; 32]>,
) -> Result<Vec<u8>> {
    let gz = GzEncoder::new(Vec::new(), GzCompression::default());
    let mut tar = TarBuilder::new(gz);

    // 1. Write room.tin
    let room_tin =
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, room).map_err(KitchenError::Tin)?;
    let (room_wire, _) = if let Some(key) = room_key {
        room_tin
            .encode_room_key(Compression::Zstd, &key, &[0u8; 12])
            .map_err(KitchenError::Tin)?
    } else {
        room_tin
            .encode_plaintext(Compression::Zstd)
            .map_err(KitchenError::Tin)?
    };

    let mut header = tar::Header::new_gnu();
    header.set_size(room_wire.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "room.tin", Cursor::new(room_wire))?;

    // 2. Write chunks
    for (tid, chunk) in chunks {
        let chunk_tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, chunk)
            .map_err(KitchenError::Tin)?;
        let (chunk_wire, _) = if let Some(key) = room_key {
            chunk_tin
                .encode_room_key(Compression::Zstd, &key, &[0u8; 12])
                .map_err(KitchenError::Tin)?
        } else {
            chunk_tin
                .encode_plaintext(Compression::Zstd)
                .map_err(KitchenError::Tin)?
        };

        let fname = format!("chunks/{}.tin", hex::encode(tid));
        let mut c_header = tar::Header::new_gnu();
        c_header.set_size(chunk_wire.len() as u64);
        c_header.set_mode(0o644);
        c_header.set_cksum();
        tar.append_data(&mut c_header, fname, Cursor::new(chunk_wire))?;
    }

    let gz_inner = tar.into_inner()?;
    let finished = gz_inner.finish()?;
    Ok(finished)
}
