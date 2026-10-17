//! Room domain model, permissions, scene orchestration, and lifecycle management.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use tin::{Compression, Tin};

use crate::cellar_sink::CellarSink;
use crate::character::{CharacterBiblio, get_character_owner};
use crate::error::{BanquetteError, Result};
use crate::labels::{KEY_STD_ADMINS, LABEL_ALHALTE_BANQUETTE, LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID};
use crate::log::{LogBiblio, LogOperationEvent, SceneLog};
use crate::scene::{Scene, SceneData};
use crate::state::SceneState;

/// Outcome of a periodic heartbeat evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatOutcome {
    /// Heartbeat succeeded, room continues operation.
    Continue,
    /// Room has been idle with no responses for 3 consecutive intervals; process should exit.
    ShouldExit,
}

/// Serialized payload for the whole room state (`alhalte/banquette`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomData {
    /// 64-bit room identifier.
    pub room_id: u64,
    /// Whether room is fully public.
    pub done: bool,
    /// Log chunk size exponent $N$.
    pub chunk_exponent: u8,
    /// Character repository.
    pub character_biblio: CharacterBiblio,
    /// Serialized scenes.
    pub scenes: HashMap<String, SceneData>,
}

/// Summary information for a scene returned to permitted callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSummary {
    /// Scene identifier string.
    pub id: String,
    /// Number of present characters.
    pub present_character_count: usize,
    /// Number of active connected clients.
    pub active_client_count: usize,
    /// Whether the scene is currently loaded in memory.
    pub is_loaded: bool,
}

/// The core session room process managing scenes, characters, and client presence.
#[derive(Debug)]
pub struct Room {
    /// 64-bit room identifier.
    pub room_id: u64,
    /// Done flag on root scene `"/"`; when `true`, all room information is fully public.
    pub done: bool,
    /// Log chunk capacity exponent $N$.
    pub chunk_exponent: u8,
    /// Scene repository mapping `scene-id` to [`Scene`].
    pub scene_biblio: HashMap<String, Scene>,
    /// Character repository mapping `character-id` to [`Character`].
    pub character_biblio: CharacterBiblio,
    /// Consecutive missed heartbeat count.
    pub missed_heartbeats: u32,
    /// Whether any client activity or heartbeat response occurred during the current cycle.
    pub client_activity_in_cycle: bool,
}

impl Room {
    /// Creates a new room initialized with root virtual scene `"/"` and the creator as admin.
    pub fn new(room_id: u64, creator_account: u64, chunk_exponent: u8) -> Result<Self> {
        let mut room = Self {
            room_id,
            done: false,
            chunk_exponent,
            scene_biblio: HashMap::new(),
            character_biblio: CharacterBiblio::new(),
            missed_heartbeats: 0,
            client_activity_in_cycle: true,
        };

        // Create root virtual scene "/"
        let mut root_scene = Scene::new("/", chunk_exponent);
        let admins = vec![creator_account];
        let admins_bytes = postcard::to_allocvec(&admins)?;
        let admins_tin = Tin::new(LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, admins_bytes);
        root_scene
            .state
            .update(KEY_STD_ADMINS.to_string(), admins_tin, true)?;

        room.scene_biblio.insert("/".to_string(), root_scene);
        Ok(room)
    }

    /// Checks if a given account ID is an administrator of the room.
    pub fn is_admin(&self, account_id: u64) -> bool {
        let Some(root) = self.scene_biblio.get("/") else {
            return false;
        };
        let Some(tin) = root.state.get(KEY_STD_ADMINS) else {
            return false;
        };
        let admins: Vec<u64> = match postcard::from_bytes(&tin.payload) {
            Ok(a) => a,
            Err(_) => return false,
        };
        admins.contains(&account_id)
    }

    /// Sets the list of administrators in `"/"` state. Requires admin privileges.
    pub fn set_admins(&mut self, caller_account: u64, admins: Vec<u64>) -> Result<()> {
        if !self.is_admin(caller_account) {
            return Err(BanquetteError::PermissionDenied);
        }
        let root = self
            .scene_biblio
            .get_mut("/")
            .ok_or_else(|| BanquetteError::SceneNotFound("/".to_string()))?;

        let admins_bytes = postcard::to_allocvec(&admins)?;
        let admins_tin = Tin::new(LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, admins_bytes);
        root.state
            .update(KEY_STD_ADMINS.to_string(), admins_tin, true)?;
        Ok(())
    }

    /// Sets the `done` flag on the room. Only administrators can change this flag.
    pub fn set_done(&mut self, caller_account: u64, done: bool) -> Result<()> {
        if !self.is_admin(caller_account) {
            return Err(BanquetteError::PermissionDenied);
        }
        self.done = done;
        Ok(())
    }

    /// Checks whether an account has access rights to query a specific scene.
    ///
    /// According to ARCHITECTURE.md:
    /// - If `done` flag is true, all room info is fully public.
    /// - Administrators have access to all scenes.
    /// - Root virtual scene `"/"` is accessible to all room members.
    /// - Otherwise, the account must own at least one character present in that scene.
    pub fn can_access_scene(&self, account_id: u64, scene_id: &str) -> bool {
        if self.done || self.is_admin(account_id) || scene_id == "/" {
            return true;
        }

        let Some(scene) = self.scene_biblio.get(scene_id) else {
            return false;
        };

        for &char_id in &scene.present_characters {
            if let Some(ch) = self.character_biblio.get(char_id)
                && get_character_owner(ch) == Some(account_id)
            {
                return true;
            }
        }

        false
    }

    /// Returns list of all scene IDs in the room (scene IDs are public information).
    pub fn list_scene_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.scene_biblio.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Queries scene summary metadata.
    ///
    /// Returns `None` without response if caller lacks permission under non-done status.
    pub fn query_scene(&self, caller_account: u64, scene_id: &str) -> Option<SceneSummary> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        let scene = self.scene_biblio.get(scene_id)?;
        Some(SceneSummary {
            id: scene.id.clone(),
            present_character_count: scene.present_characters.len(),
            active_client_count: scene.active_clients.len(),
            is_loaded: scene.is_loaded,
        })
    }

    /// Queries scene state.
    ///
    /// Returns `None` without response if caller lacks permission under non-done status.
    pub fn query_state(
        &self,
        caller_account: u64,
        scene_id: &str,
    ) -> Option<&BTreeMap<String, Tin>> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        let scene = self.scene_biblio.get(scene_id)?;
        Some(scene.state.as_map())
    }

    /// Queries scene log biblio.
    ///
    /// Returns `None` without response if caller lacks permission under non-done status.
    pub fn query_log_biblio(&self, caller_account: u64, scene_id: &str) -> Option<&LogBiblio> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        let scene = self.scene_biblio.get(scene_id)?;
        Some(&scene.log.biblio)
    }

    /// Queries a chunk of log entries.
    ///
    /// Returns `None` without response if caller lacks permission under non-done status.
    pub async fn query_log_chunk(
        &self,
        caller_account: u64,
        scene_id: &str,
        chunk_index: u32,
        cellar: &dyn CellarSink,
    ) -> Option<Vec<Option<Tin>>> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        let scene = self.scene_biblio.get(scene_id)?;
        scene
            .log
            .read_chunk(chunk_index, cellar, Some(caller_account))
            .await
            .ok()
    }

    /// Creates a new scene in the room.
    pub fn create_scene(
        &mut self,
        _caller_account: u64,
        scene_id: &str,
        inherit_juices_from: Option<&str>,
    ) -> Result<()> {
        if self.scene_biblio.contains_key(scene_id) {
            return Ok(());
        }

        let mut new_scene = Scene::new(scene_id, self.chunk_exponent);

        // Inherit juices (e.g. background image and BGM) if requested
        if let Some(source) = inherit_juices_from.and_then(|id| self.scene_biblio.get(id)) {
            for (k, v) in source.state.as_map() {
                if k.starts_with("std::") && k != KEY_STD_ADMINS {
                    let _ = new_scene.state.update(k.clone(), v.clone(), true);
                }
            }
        }

        self.scene_biblio.insert(scene_id.to_string(), new_scene);
        Ok(())
    }

    /// Deletes a scene. Only administrators can delete scenes (cannot delete root scene `"/""`).
    pub fn delete_scene(&mut self, caller_account: u64, scene_id: &str) -> Result<()> {
        if !self.is_admin(caller_account) {
            return Err(BanquetteError::PermissionDenied);
        }
        if scene_id == "/" {
            return Err(BanquetteError::PermissionDenied);
        }

        self.scene_biblio
            .remove(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;
        Ok(())
    }

    /// Updates scene state.
    ///
    /// Only administrators can modify `"/"` state.
    pub fn update_scene_state(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        key: String,
        value: Tin,
    ) -> Result<Option<Tin>> {
        let is_admin = self.is_admin(caller_account);
        if scene_id == "/" && !is_admin {
            return Err(BanquetteError::PermissionDenied);
        }

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene.state.update(key, value, is_admin)
    }

    /// Deletes a key from scene state.
    pub fn delete_scene_state(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        key: &str,
    ) -> Result<Option<Tin>> {
        let is_admin = self.is_admin(caller_account);
        if scene_id == "/" && !is_admin {
            return Err(BanquetteError::PermissionDenied);
        }

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene.state.delete(key, is_admin)
    }

    /// Appends a log entry to a scene.
    pub async fn append_log(
        &mut self,
        caller_account: u64,
        session_id: u64,
        scene_id: &str,
        entry: Option<Tin>,
        cellar: &dyn CellarSink,
    ) -> Result<LogOperationEvent> {
        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene
            .log
            .append(entry, cellar, caller_account, session_id)
            .await
    }

    /// Moves a character to a target scene.
    pub fn move_character(
        &mut self,
        caller_account: u64,
        character_id: u64,
        to_scene_id: &str,
    ) -> Result<()> {
        let is_admin = self.is_admin(caller_account);
        let ch = self
            .character_biblio
            .get(character_id)
            .ok_or(BanquetteError::CharacterNotFound(character_id))?;

        if !is_admin && get_character_owner(ch) != Some(caller_account) {
            return Err(BanquetteError::PermissionDenied);
        }

        if !self.scene_biblio.contains_key(to_scene_id) {
            self.create_scene(caller_account, to_scene_id, Some("/"))?;
        }

        // Remove from any current scenes
        for scene in self.scene_biblio.values_mut() {
            scene.present_characters.remove(&character_id);
        }

        // Add to new scene
        if let Some(target) = self.scene_biblio.get_mut(to_scene_id) {
            target.present_characters.insert(character_id);
        }

        Ok(())
    }

    /// Handles client entering a scene.
    ///
    /// If the scene was unloaded, restores it from cellar.
    pub async fn client_enter_scene(
        &mut self,
        client_id: u64,
        caller_account: u64,
        scene_id: &str,
        cellar: &dyn CellarSink,
    ) -> Result<()> {
        self.client_activity_in_cycle = true;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        if !scene.is_loaded {
            scene.reload_if_needed(cellar, Some(caller_account)).await?;
        }

        scene.active_clients.insert(client_id);
        Ok(())
    }

    /// Handles client leaving a scene.
    ///
    /// If active clients drops to 0 and the scene is not `"/"`, persists and unloads the scene.
    pub async fn client_leave_scene(
        &mut self,
        client_id: u64,
        caller_account: u64,
        session_id: u64,
        scene_id: &str,
        cellar: &dyn CellarSink,
    ) -> Result<()> {
        self.client_activity_in_cycle = true;

        let Some(scene) = self.scene_biblio.get_mut(scene_id) else {
            return Ok(());
        };

        scene.active_clients.remove(&client_id);

        if scene.active_clients.is_empty() && scene_id != "/" && scene.is_loaded {
            scene
                .persist_and_unload(cellar, caller_account, session_id)
                .await?;
        }

        Ok(())
    }

    /// Removes a disconnected client from all scenes, triggering unloads where appropriate.
    pub async fn client_disconnect(
        &mut self,
        client_id: u64,
        caller_account: u64,
        session_id: u64,
        cellar: &dyn CellarSink,
    ) -> Result<()> {
        let scene_ids: Vec<String> = self.scene_biblio.keys().cloned().collect();
        for sid in scene_ids {
            let _ = self
                .client_leave_scene(client_id, caller_account, session_id, &sid, cellar)
                .await;
        }
        Ok(())
    }

    /// Records that client activity occurred in the current heartbeat interval.
    pub fn record_client_activity(&mut self) {
        self.client_activity_in_cycle = true;
    }

    /// Advances one heartbeat cycle.
    ///
    /// - Serializes the whole room state into a `Tin` with label `alhalte/banquette` and saves it to cellar.
    /// - Checks client activity. If no client activity occurred during the cycle, increments `missed_heartbeats`.
    /// - If 3 consecutive missed heartbeats are accumulated, returns [`HeartbeatOutcome::ShouldExit`].
    pub async fn step_heartbeat(
        &mut self,
        cellar: &dyn CellarSink,
        owner: u64,
        session_id: u64,
    ) -> Result<HeartbeatOutcome> {
        // Persist whole room state
        let room_tin = self.to_tin()?;
        let (wire_bytes, tin_id) = room_tin
            .encode_plaintext(Compression::Zstd)
            .map_err(BanquetteError::Tin)?;

        cellar
            .store_tin(owner, session_id, &wire_bytes, &tin_id, None)
            .await?;

        // Evaluate activity
        let had_activity = self.client_activity_in_cycle;
        self.client_activity_in_cycle = false;

        // Also check if any clients are currently connected
        let any_connected_clients = self
            .scene_biblio
            .values()
            .any(|s| !s.active_clients.is_empty());

        if had_activity || any_connected_clients {
            self.missed_heartbeats = 0;
            Ok(HeartbeatOutcome::Continue)
        } else {
            self.missed_heartbeats = self.missed_heartbeats.saturating_add(1);
            if self.missed_heartbeats >= 3 {
                Ok(HeartbeatOutcome::ShouldExit)
            } else {
                Ok(HeartbeatOutcome::Continue)
            }
        }
    }

    /// Serializes whole room state into a `Tin` with label `alhalte/banquette`.
    pub fn to_tin(&self) -> Result<Tin> {
        let mut scenes_data = HashMap::new();
        for (sid, scene) in &self.scene_biblio {
            scenes_data.insert(
                sid.clone(),
                SceneData {
                    id: scene.id.clone(),
                    states: scene.state.as_map().clone(),
                    biblio: scene.log.biblio.clone(),
                    present_characters: scene.present_characters.iter().copied().collect(),
                },
            );
        }

        let data = RoomData {
            room_id: self.room_id,
            done: self.done,
            chunk_exponent: self.chunk_exponent,
            character_biblio: self.character_biblio.clone(),
            scenes: scenes_data,
        };

        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &data).map_err(BanquetteError::Tin)
    }

    /// Reconstructs a [`Room`] from a `Tin` with label `alhalte/banquette`.
    pub fn from_tin(tin: &Tin) -> Result<Self> {
        if tin.label != LABEL_ALHALTE_BANQUETTE {
            return Err(BanquetteError::Tin(tin::TinError::LabelMismatch {
                expected: LABEL_ALHALTE_BANQUETTE,
                actual: tin.label,
            }));
        }

        let data: RoomData = tin.to_postcard().map_err(BanquetteError::Tin)?;
        let mut scene_biblio = HashMap::new();

        for (sid, sdata) in data.scenes {
            let state = SceneState::from_entries(sdata.states)?;
            let mut log = SceneLog::new(data.chunk_exponent);
            log.biblio = sdata.biblio;
            let present_characters = sdata.present_characters.into_iter().collect();

            let scene = Scene {
                id: sid.clone(),
                state,
                log,
                present_characters,
                active_clients: HashSet::new(),
                is_loaded: true,
                persisted_tin_id: None,
            };
            scene_biblio.insert(sid, scene);
        }

        Ok(Self {
            room_id: data.room_id,
            done: data.done,
            chunk_exponent: data.chunk_exponent,
            scene_biblio,
            character_biblio: data.character_biblio,
            missed_heartbeats: 0,
            client_activity_in_cycle: true,
        })
    }
}
