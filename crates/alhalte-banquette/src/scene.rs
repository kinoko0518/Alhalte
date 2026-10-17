//! Scene domain model, access control, and scene state persistence.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use tin::{Compression, Tin, TinId};

use crate::cellar_sink::CellarSink;
use crate::error::{BanquetteError, Result};
use crate::labels::LABEL_ALHALTE_BANQUETTE_SCENE;
use crate::log::{LogBiblio, SceneLog};
use crate::state::SceneState;

/// Postcard-serializable payload for label `alhalte/banquette/scene`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneData {
    /// Scene identifier (e.g. `"/"`, `"field"`, `"tavern"`).
    pub id: String,
    /// Scene states map.
    pub states: BTreeMap<String, Tin>,
    /// Log chunk connection and head biblio.
    pub biblio: LogBiblio,
    /// Characters currently present in this scene.
    pub present_characters: Vec<u64>,
}

/// An individual scene with independent log, state, and character presence.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Unique scene identifier UTF-8 string.
    pub id: String,
    /// Scene state map.
    pub state: SceneState,
    /// Scene log manager.
    pub log: SceneLog,
    /// Set of character IDs currently present in this scene.
    pub present_characters: HashSet<u64>,
    /// Set of currently connected active client session/connection IDs in this scene.
    pub active_clients: HashSet<u64>,
    /// Whether this scene is currently loaded in memory.
    pub is_loaded: bool,
    /// Most recent persisted cellar tin ID if unloaded.
    pub persisted_tin_id: Option<TinId>,
}

impl Scene {
    /// Creates a new loaded [`Scene`] with the given ID and log chunk exponent.
    pub fn new(id: impl Into<String>, chunk_exponent: u8) -> Self {
        Self {
            id: id.into(),
            state: SceneState::new(),
            log: SceneLog::new(chunk_exponent),
            present_characters: HashSet::new(),
            active_clients: HashSet::new(),
            is_loaded: true,
            persisted_tin_id: None,
        }
    }

    /// Serializes this scene's state and biblio into a `Tin` with label `alhalte/banquette/scene`.
    pub fn to_tin(&self) -> Result<Tin> {
        let data = SceneData {
            id: self.id.clone(),
            states: self.state.as_map().clone(),
            biblio: self.log.biblio.clone(),
            present_characters: self.present_characters.iter().copied().collect(),
        };
        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE_SCENE, &data).map_err(BanquetteError::Tin)
    }

    /// Reconstructs a [`Scene`] from a `Tin` with label `alhalte/banquette/scene`.
    pub fn from_tin(tin: &Tin, chunk_exponent: u8) -> Result<Self> {
        if tin.label != LABEL_ALHALTE_BANQUETTE_SCENE {
            return Err(BanquetteError::Tin(tin::TinError::LabelMismatch {
                expected: LABEL_ALHALTE_BANQUETTE_SCENE,
                actual: tin.label,
            }));
        }

        let data: SceneData = tin.to_postcard().map_err(BanquetteError::Tin)?;
        let state = SceneState::from_entries(data.states)?;
        let mut log = SceneLog::new(chunk_exponent);
        log.biblio = data.biblio;

        let present_characters = data.present_characters.into_iter().collect();

        Ok(Self {
            id: data.id,
            state,
            log,
            present_characters,
            active_clients: HashSet::new(),
            is_loaded: true,
            persisted_tin_id: None,
        })
    }

    /// Persists this scene into cellar storage and unloads its memory footprint.
    pub async fn persist_and_unload(
        &mut self,
        cellar: &dyn CellarSink,
        owner: u64,
        session_id: u64,
    ) -> Result<TinId> {
        let tin = self.to_tin()?;
        let (wire_bytes, tin_id) = tin
            .encode_plaintext(Compression::Zstd)
            .map_err(BanquetteError::Tin)?;

        cellar
            .store_tin(owner, session_id, &wire_bytes, &tin_id, None)
            .await?;

        self.persisted_tin_id = Some(tin_id);
        self.is_loaded = false;
        self.log.active_chunk.clear();
        self.log.sealed_cache.clear();

        Ok(tin_id)
    }

    /// Reloads an unloaded scene from cellar on-demand.
    pub async fn reload_if_needed(
        &mut self,
        cellar: &dyn CellarSink,
        owner: Option<u64>,
    ) -> Result<()> {
        if self.is_loaded {
            return Ok(());
        }

        let tin_id = self
            .persisted_tin_id
            .ok_or_else(|| BanquetteError::Cellar("No persisted tin ID for scene".to_string()))?;

        let bytes = cellar.download_tin(&tin_id, owner).await?;
        let (tin, _, _) = Tin::decode(&bytes, None).map_err(BanquetteError::Tin)?;
        let restored = Self::from_tin(&tin, self.log.exponent)?;

        self.state = restored.state;
        self.log.biblio = restored.log.biblio;
        self.present_characters = restored.present_characters;
        self.is_loaded = true;

        Ok(())
    }
}
