//! Log chunk management, biblio definitions, and log event helpers.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use tin::{Compression, Tin, TinId};

use crate::cellar_sink::CellarSink;
use crate::error::{BanquetteError, Result};
use crate::labels::{
    LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME,
    LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE, LABEL_ALHALTE_BANQUETTE_LOG_CHUNK,
};

/// Connection definition for sealed log chunks and the current write/read head.
///
/// Label: `alhalte/banquette/log/biblio`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogBiblio {
    /// Ordered list of 256-bit tin IDs for sealed historical chunks stored in cellar.
    pub chunks: Vec<TinId>,
    /// Current head pointer: `(chunk_index, position_in_chunk)`.
    pub head: (u32, u32),
}

impl LogBiblio {
    /// Creates a new initial [`LogBiblio`] starting at `head = (0, 0)` with no sealed chunks.
    pub const fn new() -> Self {
        Self {
            chunks: Vec::new(),
            head: (0, 0),
        }
    }

    /// Serializes this biblio into a `Tin` with label `alhalte/banquette/log/biblio`.
    pub fn to_tin(&self) -> Result<Tin> {
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO, self)
            .map_err(BanquetteError::Tin)
    }

    /// Deserializes a `LogBiblio` from a `Tin`.
    pub fn from_tin(tin: &Tin) -> Result<Self> {
        if tin.label != LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO {
            return Err(BanquetteError::Tin(tin::TinError::LabelMismatch {
                expected: LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO,
                actual: tin.label,
            }));
        }
        tin.to_postcard().map_err(BanquetteError::Tin)
    }
}

impl Default for LogBiblio {
    fn default() -> Self {
        Self::new()
    }
}

/// A chunk of log entries containing up to `1 << N` items.
///
/// Label: `alhalte/banquette/log/chunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogChunk {
    /// Log entries. `None` represents a tombstone.
    pub entries: Vec<Option<Tin>>,
}

impl LogChunk {
    /// Creates a new [`LogChunk`] from a list of entries.
    pub const fn new(entries: Vec<Option<Tin>>) -> Self {
        Self { entries }
    }

    /// Serializes this chunk into a `Tin` with label `alhalte/banquette/log/chunk`.
    pub fn to_tin(&self) -> Result<Tin> {
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, self)
            .map_err(BanquetteError::Tin)
    }

    /// Deserializes a `LogChunk` from a `Tin`.
    pub fn from_tin(tin: &Tin) -> Result<Self> {
        if tin.label != LABEL_ALHALTE_BANQUETTE_LOG_CHUNK {
            return Err(BanquetteError::Tin(tin::TinError::LabelMismatch {
                expected: LABEL_ALHALTE_BANQUETTE_LOG_CHUNK,
                actual: tin.label,
            }));
        }
        tin.to_postcard().map_err(BanquetteError::Tin)
    }
}

/// Returns the current UNIX time in whole seconds.
///
/// Returns `0` when the system clock is set before the UNIX epoch. Reading the
/// clock must never panic, so the error is folded into the fallback instead of
/// being unwrapped.
pub fn unix_timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Character entered scene event.
///
/// Label: `alhalte/banquette/log/character-came`.
///
/// Wire compatibility (Issue #227): the current postcard payload is
/// `character_id` followed by `timestamp` (two `u64` varints in that order).
/// Events persisted before the timestamp was added carry only the single
/// `character_id` varint; [`Self::from_tin`] still decodes them and yields
/// `timestamp: 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterCameEvent {
    /// 64-bit character ID.
    pub character_id: u64,
    /// UNIX time (whole seconds) at which the entry was appended.
    pub timestamp: u64,
}

impl CharacterCameEvent {
    /// Creates an event stamped with the current UNIX time.
    pub fn now(character_id: u64) -> Self {
        Self {
            character_id,
            timestamp: unix_timestamp_now(),
        }
    }

    /// Encodes this event as a `Tin`.
    ///
    /// The payload is the postcard serialization of `(character_id, timestamp)`.
    /// Readers must also accept the legacy id-only payload (see
    /// [`Self::from_tin`]).
    pub fn to_tin(&self) -> Result<Tin> {
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, self)
            .map_err(BanquetteError::Tin)
    }

    /// Decodes this event from a `Tin`.
    ///
    /// Tries the current `character_id` + `timestamp` payload first and falls
    /// back to the legacy payload that carries only `character_id`, decoding it
    /// with `timestamp: 0`. A payload that decodes in neither form is an error.
    pub fn from_tin(tin: &Tin) -> Result<Self> {
        if tin.label != LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME {
            return Err(BanquetteError::Tin(tin::TinError::LabelMismatch {
                expected: LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME,
                actual: tin.label,
            }));
        }
        match tin.to_postcard() {
            Ok(event) => Ok(event),
            Err(_) => {
                let character_id: u64 = tin.to_postcard().map_err(BanquetteError::Tin)?;
                Ok(Self {
                    character_id,
                    timestamp: 0,
                })
            }
        }
    }

    /// Duck-typing helper converting this event to `HashMap<String, Tin>`.
    ///
    /// The map holds `character_id` and `timestamp` as primitive `u64` tins.
    pub fn to_duck_typed_map(&self) -> HashMap<String, Tin> {
        let mut map = HashMap::new();
        map.insert("character_id".to_string(), Tin::from_u64(self.character_id));
        map.insert("timestamp".to_string(), Tin::from_u64(self.timestamp));
        map
    }

    /// Duck-typing helper reconstructing this event from `HashMap<String, Tin>`.
    ///
    /// A missing `timestamp` key (a map written before Issue #227) decodes as
    /// `0`, matching [`Self::from_tin`]'s legacy fallback.
    pub fn from_duck_typed_map(map: &HashMap<String, Tin>) -> Result<Self> {
        let t = map
            .get("character_id")
            .ok_or_else(|| BanquetteError::Serialization("missing character_id".to_string()))?;
        let character_id = t.as_u64().map_err(BanquetteError::Tin)?;
        let timestamp = match map.get("timestamp") {
            Some(t) => t.as_u64().map_err(BanquetteError::Tin)?,
            None => 0,
        };
        Ok(Self {
            character_id,
            timestamp,
        })
    }
}

/// Character left scene event.
///
/// Label: `alhalte/banquette/log/character-gone`.
///
/// Wire compatibility (Issue #227): like [`CharacterCameEvent`], the current
/// payload is `character_id` followed by `timestamp`; legacy id-only payloads
/// decode with `timestamp: 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterGoneEvent {
    /// 64-bit character ID.
    pub character_id: u64,
    /// UNIX time (whole seconds) at which the entry was appended.
    pub timestamp: u64,
}

impl CharacterGoneEvent {
    /// Creates an event stamped with the current UNIX time.
    pub fn now(character_id: u64) -> Self {
        Self {
            character_id,
            timestamp: unix_timestamp_now(),
        }
    }

    /// Encodes this event as a `Tin`.
    ///
    /// The payload is the postcard serialization of `(character_id, timestamp)`.
    /// Readers must also accept the legacy id-only payload (see
    /// [`Self::from_tin`]).
    pub fn to_tin(&self) -> Result<Tin> {
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE, self)
            .map_err(BanquetteError::Tin)
    }

    /// Decodes this event from a `Tin`.
    ///
    /// Tries the current `character_id` + `timestamp` payload first and falls
    /// back to the legacy payload that carries only `character_id`, decoding it
    /// with `timestamp: 0`. A payload that decodes in neither form is an error.
    pub fn from_tin(tin: &Tin) -> Result<Self> {
        if tin.label != LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE {
            return Err(BanquetteError::Tin(tin::TinError::LabelMismatch {
                expected: LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
                actual: tin.label,
            }));
        }
        match tin.to_postcard() {
            Ok(event) => Ok(event),
            Err(_) => {
                let character_id: u64 = tin.to_postcard().map_err(BanquetteError::Tin)?;
                Ok(Self {
                    character_id,
                    timestamp: 0,
                })
            }
        }
    }

    /// Duck-typing helper converting this event to `HashMap<String, Tin>`.
    ///
    /// The map holds `character_id` and `timestamp` as primitive `u64` tins.
    pub fn to_duck_typed_map(&self) -> HashMap<String, Tin> {
        let mut map = HashMap::new();
        map.insert("character_id".to_string(), Tin::from_u64(self.character_id));
        map.insert("timestamp".to_string(), Tin::from_u64(self.timestamp));
        map
    }

    /// Duck-typing helper reconstructing this event from `HashMap<String, Tin>`.
    ///
    /// A missing `timestamp` key (a map written before Issue #227) decodes as
    /// `0`, matching [`Self::from_tin`]'s legacy fallback.
    pub fn from_duck_typed_map(map: &HashMap<String, Tin>) -> Result<Self> {
        let t = map
            .get("character_id")
            .ok_or_else(|| BanquetteError::Serialization("missing character_id".to_string()))?;
        let character_id = t.as_u64().map_err(BanquetteError::Tin)?;
        let timestamp = match map.get("timestamp") {
            Some(t) => t.as_u64().map_err(BanquetteError::Tin)?,
            None => 0,
        };
        Ok(Self {
            character_id,
            timestamp,
        })
    }
}

/// Notification event dispatched to scene clients when the log is modified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogOperationEvent {
    /// A new entry was appended to the log head.
    Appended {
        /// Position `(chunk_index, position_in_chunk)`.
        position: (u32, u32),
        /// The appended entry (or tombstone).
        entry: Option<Tin>,
    },
    /// An existing entry before the head was edited.
    Edited {
        /// Position `(chunk_index, position_in_chunk)`.
        position: (u32, u32),
        /// The new entry value.
        entry: Tin,
    },
    /// An existing entry before the head was deleted (tombstone).
    Deleted {
        /// Position `(chunk_index, position_in_chunk)`.
        position: (u32, u32),
    },
    /// The head was rewound back to an earlier position.
    Rewound {
        /// The new head position.
        to_position: (u32, u32),
    },
}

/// Scene log manager handling chunking, head tracking, and persistence to cellar.
#[derive(Debug, Clone)]
pub struct SceneLog {
    /// Chunk size exponent $N$. Capacity of one chunk is `1 << exponent`.
    pub exponent: u8,
    /// Biblio describing sealed chunks in cellar and the current head.
    pub biblio: LogBiblio,
    /// Current unsealed chunk buffer.
    pub active_chunk: Vec<Option<Tin>>,
    /// In-memory cache of historical sealed chunks for fast reading and editing.
    pub sealed_cache: HashMap<u32, Vec<Option<Tin>>>,
}

impl SceneLog {
    /// Creates a new empty [`SceneLog`] with the given chunk size exponent $N$.
    pub fn new(exponent: u8) -> Self {
        Self {
            exponent,
            biblio: LogBiblio::new(),
            active_chunk: Vec::new(),
            sealed_cache: HashMap::new(),
        }
    }

    /// Returns the maximum capacity of entries per chunk (`1 << exponent`).
    pub fn chunk_capacity(&self) -> usize {
        1usize << self.exponent
    }

    /// Current head position: `(chunk_index, position_in_chunk)`.
    pub fn head(&self) -> (u32, u32) {
        self.biblio.head
    }

    /// Appends a new entry to the log.
    ///
    /// If the active chunk reaches its capacity (`1 << exponent`), it is sealed into a `Tin`,
    /// uploaded to `cellar`, and a new empty chunk is started.
    pub async fn append(
        &mut self,
        entry: Option<Tin>,
        cellar: &dyn CellarSink,
        owner: u64,
        session_id: u64,
    ) -> Result<LogOperationEvent> {
        let (chunk_idx, pos) = self.biblio.head;
        let event_pos = (chunk_idx, pos);

        // Append to active chunk
        self.active_chunk.push(entry.clone());
        let next_pos = pos + 1;

        if (next_pos as usize) >= self.chunk_capacity() {
            // Chunk is full; seal and upload
            let chunk_obj = LogChunk::new(std::mem::take(&mut self.active_chunk));
            let chunk_tin = chunk_obj.to_tin()?;
            let (wire_bytes, tin_id) = chunk_tin
                .encode_plaintext(Compression::Zstd)
                .map_err(BanquetteError::Tin)?;

            cellar
                .store_tin(owner, session_id, &wire_bytes, &tin_id, None)
                .await?;

            self.sealed_cache.insert(chunk_idx, chunk_obj.entries);
            self.biblio.chunks.push(tin_id);
            self.biblio.head = (chunk_idx + 1, 0);
        } else {
            self.biblio.head = (chunk_idx, next_pos);
        }

        Ok(LogOperationEvent::Appended {
            position: event_pos,
            entry,
        })
    }

    /// Edits an entry strictly before the head with a new `Some(Tin)`.
    pub async fn edit(
        &mut self,
        chunk: u32,
        pos: u32,
        entry: Tin,
        cellar: &dyn CellarSink,
        owner: u64,
        session_id: u64,
    ) -> Result<LogOperationEvent> {
        let (head_chunk, head_pos) = self.biblio.head;
        if chunk > head_chunk || (chunk == head_chunk && pos >= head_pos) {
            return Err(BanquetteError::InvalidLogPosition {
                chunk,
                position: pos,
                head_chunk,
                head_position: head_pos,
            });
        }

        if chunk == head_chunk {
            if (pos as usize) < self.active_chunk.len() {
                self.active_chunk[pos as usize] = Some(entry.clone());
            } else {
                return Err(BanquetteError::InvalidLogPosition {
                    chunk,
                    position: pos,
                    head_chunk,
                    head_position: head_pos,
                });
            }
        } else {
            // Edit in a past chunk
            let entries = self.load_chunk_mut(chunk, cellar, owner).await?;
            if (pos as usize) < entries.len() {
                entries[pos as usize] = Some(entry.clone());
                // Re-seal and update cellar reference
                let chunk_obj = LogChunk::new(entries.clone());
                let chunk_tin = chunk_obj.to_tin()?;
                let (wire_bytes, tin_id) = chunk_tin
                    .encode_plaintext(Compression::Zstd)
                    .map_err(BanquetteError::Tin)?;
                cellar
                    .store_tin(owner, session_id, &wire_bytes, &tin_id, None)
                    .await?;

                if (chunk as usize) < self.biblio.chunks.len() {
                    self.biblio.chunks[chunk as usize] = tin_id;
                }
            } else {
                return Err(BanquetteError::InvalidLogPosition {
                    chunk,
                    position: pos,
                    head_chunk,
                    head_position: head_pos,
                });
            }
        }

        Ok(LogOperationEvent::Edited {
            position: (chunk, pos),
            entry,
        })
    }

    /// Deletes an entry strictly before the head (replaces with tombstone `None`).
    pub async fn delete(
        &mut self,
        chunk: u32,
        pos: u32,
        cellar: &dyn CellarSink,
        owner: u64,
        session_id: u64,
    ) -> Result<LogOperationEvent> {
        let (head_chunk, head_pos) = self.biblio.head;
        if chunk > head_chunk || (chunk == head_chunk && pos >= head_pos) {
            return Err(BanquetteError::InvalidLogPosition {
                chunk,
                position: pos,
                head_chunk,
                head_position: head_pos,
            });
        }

        if chunk == head_chunk {
            if (pos as usize) < self.active_chunk.len() {
                self.active_chunk[pos as usize] = None;
            } else {
                return Err(BanquetteError::InvalidLogPosition {
                    chunk,
                    position: pos,
                    head_chunk,
                    head_position: head_pos,
                });
            }
        } else {
            let entries = self.load_chunk_mut(chunk, cellar, owner).await?;
            if (pos as usize) < entries.len() {
                entries[pos as usize] = None;
                // Re-seal and update cellar reference
                let chunk_obj = LogChunk::new(entries.clone());
                let chunk_tin = chunk_obj.to_tin()?;
                let (wire_bytes, tin_id) = chunk_tin
                    .encode_plaintext(Compression::Zstd)
                    .map_err(BanquetteError::Tin)?;
                cellar
                    .store_tin(owner, session_id, &wire_bytes, &tin_id, None)
                    .await?;

                if (chunk as usize) < self.biblio.chunks.len() {
                    self.biblio.chunks[chunk as usize] = tin_id;
                }
            } else {
                return Err(BanquetteError::InvalidLogPosition {
                    chunk,
                    position: pos,
                    head_chunk,
                    head_position: head_pos,
                });
            }
        }

        Ok(LogOperationEvent::Deleted {
            position: (chunk, pos),
        })
    }

    /// Rewinds the log head back to `(chunk, pos)`.
    ///
    /// Any entries after the new head become undefined and are truncated.
    pub fn rewind(&mut self, chunk: u32, pos: u32) -> Result<LogOperationEvent> {
        let (head_chunk, head_pos) = self.biblio.head;
        if chunk > head_chunk || (chunk == head_chunk && pos > head_pos) {
            return Err(BanquetteError::InvalidLogPosition {
                chunk,
                position: pos,
                head_chunk,
                head_position: head_pos,
            });
        }

        if chunk == head_chunk {
            self.active_chunk.truncate(pos as usize);
        } else {
            // Truncate biblio chunks and caches beyond `chunk`
            self.biblio.chunks.truncate(chunk as usize);
            self.sealed_cache.retain(|&c, _| c < chunk);
            // In the target chunk, we restore it as the active chunk truncated to `pos`
            if let Some(mut entries) = self.sealed_cache.remove(&chunk) {
                entries.truncate(pos as usize);
                self.active_chunk = entries;
            } else {
                self.active_chunk.clear();
            }
        }

        self.biblio.head = (chunk, pos);

        Ok(LogOperationEvent::Rewound {
            to_position: (chunk, pos),
        })
    }

    /// Reads all entries for a specific chunk (from active chunk or sealed storage).
    pub async fn read_chunk(
        &self,
        chunk: u32,
        cellar: &dyn CellarSink,
        owner: Option<u64>,
    ) -> Result<Vec<Option<Tin>>> {
        let (head_chunk, _) = self.biblio.head;
        if chunk == head_chunk {
            Ok(self.active_chunk.clone())
        } else if let Some(entries) = self.sealed_cache.get(&chunk) {
            Ok(entries.clone())
        } else if (chunk as usize) < self.biblio.chunks.len() {
            let tin_id = &self.biblio.chunks[chunk as usize];
            let raw_bytes = cellar.download_tin(tin_id, owner).await?;
            let (tin, _, _) = Tin::decode(&raw_bytes, None).map_err(BanquetteError::Tin)?;
            let chunk_obj = LogChunk::from_tin(&tin)?;
            Ok(chunk_obj.entries)
        } else {
            Err(BanquetteError::InvalidLogPosition {
                chunk,
                position: 0,
                head_chunk,
                head_position: self.biblio.head.1,
            })
        }
    }

    async fn load_chunk_mut(
        &mut self,
        chunk: u32,
        cellar: &dyn CellarSink,
        owner: u64,
    ) -> Result<&mut Vec<Option<Tin>>> {
        if !self.sealed_cache.contains_key(&chunk) {
            if (chunk as usize) < self.biblio.chunks.len() {
                let tin_id = &self.biblio.chunks[chunk as usize];
                let raw_bytes = cellar.download_tin(tin_id, Some(owner)).await?;
                let (tin, _, _) = Tin::decode(&raw_bytes, None).map_err(BanquetteError::Tin)?;
                let chunk_obj = LogChunk::from_tin(&tin)?;
                self.sealed_cache.insert(chunk, chunk_obj.entries);
            } else {
                return Err(BanquetteError::InvalidLogPosition {
                    chunk,
                    position: 0,
                    head_chunk: self.biblio.head.0,
                    head_position: self.biblio.head.1,
                });
            }
        }
        self.sealed_cache.get_mut(&chunk).ok_or_else(|| {
            BanquetteError::Cellar(format!("Failed to retrieve cached chunk {chunk}"))
        })
    }
}
