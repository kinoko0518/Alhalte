//! Room domain model, permissions, scene orchestration, and lifecycle management.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use tin::{Compression, Tin, TinId};

use crate::cellar_sink::{CellarSink, INTERNAL_WRITE_SESSION_ID};
use crate::character::{CharacterBiblio, get_character_owner};
use crate::error::{BanquetteError, Result};
use crate::labels::{
    KEY_STD_ADMINS, KEY_STD_NAME, LABEL_ALHALTE_BANQUETTE, LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID,
};
use crate::log::{LogBiblio, LogChunk, LogOperationEvent, SceneLog};
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

/// Result of one [`Room::step_heartbeat`] cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatResult {
    /// Liveness decision for this cycle.
    pub outcome: HeartbeatOutcome,
    /// Tin ID of the `RoomData` tin stored to cellar during this cycle.
    ///
    /// `None` when the store failed. `maitre-d` only advances the persisted room
    /// head pointer after a successful store, so a failed cycle must not report
    /// one.
    pub persisted_tin: Option<TinId>,
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
///
/// This is a protocol summary (broadcast as `SceneList` / `SceneInfo`), not a
/// persisted structure: `RoomData` / `SceneData` keep their existing wire
/// schema and continue to persist membership as
/// [`crate::scene::SceneData::present_characters`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSummary {
    /// Scene identifier string.
    pub id: String,
    /// Human-readable scene name (`std::name`), falling back to the scene ID.
    pub name: String,
    /// Number of present characters.
    pub present_character_count: usize,
    /// Number of active connected clients.
    pub active_client_count: usize,
    /// Whether the scene is currently loaded in memory.
    pub is_loaded: bool,
    /// IDs of the characters currently present in this scene, sorted ascending.
    ///
    /// Clients use this to rebuild `scene.members` on reload / reconnect instead
    /// of guessing membership from the character list.
    pub present_characters: Vec<u64>,
}

/// Default human-readable name of the root virtual scene `"/"`.
pub const DEFAULT_ROOT_SCENE_NAME: &str = "メインシーン";

/// Reads the human-readable scene name from a scene state map.
///
/// The value may be a `primitive/string` Tin or any other Tin whose payload is
/// valid UTF-8 (older writers did not encode the label consistently). An empty
/// or missing name falls back to the root default for `"/"` and to the scene ID
/// otherwise, so `SceneList` always renders.
fn scene_name_or_id(scene: &Scene) -> String {
    scene
        .state
        .get(KEY_STD_NAME)
        .and_then(|tin| std::str::from_utf8(&tin.payload).ok())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            if scene.id == "/" {
                DEFAULT_ROOT_SCENE_NAME.to_string()
            } else {
                scene.id.clone()
            }
        })
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
    /// Tin ID of the most recently stored (or restored) `RoomData` head.
    ///
    /// Used for retention: when a newer head is stored, this tin is
    /// unregistered from cellar on a best-effort basis so GC can reclaim it.
    pub head_tin: Option<TinId>,
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
            head_tin: None,
        };

        // Create root virtual scene "/"
        let mut root_scene = Scene::new("/", chunk_exponent);
        root_scene.state.update(
            KEY_STD_NAME.to_string(),
            Tin::from_string(DEFAULT_ROOT_SCENE_NAME),
            true,
        )?;
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
        self.apply_admin_set(admins)
    }

    /// Replaces the administrator set from an internal control frame.
    ///
    /// This is the `maitre-d` control path used by
    /// [`crate::protocol::MaitreDToBanquette::SetAdmins`]. Authorization already
    /// happened against the database `room_permissions` rows, so no room-client
    /// caller check is performed here. It is intentionally unreachable from a
    /// [`crate::protocol::ClientFrame`]; clients keep using
    /// [`Room::set_admins`], which enforces the admin check.
    pub fn apply_admin_set(&mut self, admins: Vec<u64>) -> Result<()> {
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

    /// Builds the summary of a scene without a caller permission check.
    ///
    /// Scene summaries are public room information (they only expose the ID,
    /// the `std::name` value, and presence counts) and are broadcast as
    /// `SceneList` / `SceneInfo`.
    fn summary_of(scene: &Scene) -> SceneSummary {
        let mut present_characters: Vec<u64> = scene.present_characters.iter().copied().collect();
        present_characters.sort_unstable();
        SceneSummary {
            id: scene.id.clone(),
            name: scene_name_or_id(scene),
            present_character_count: present_characters.len(),
            active_client_count: scene.active_clients.len(),
            is_loaded: scene.is_loaded,
            present_characters,
        }
    }

    /// Returns the summary of a scene by ID, or `None` when it does not exist.
    pub fn scene_summary(&self, scene_id: &str) -> Option<SceneSummary> {
        self.scene_biblio.get(scene_id).map(Self::summary_of)
    }

    /// Returns summaries of every scene in the room, sorted by scene ID.
    pub fn list_scene_summaries(&self) -> Vec<SceneSummary> {
        let mut summaries: Vec<SceneSummary> =
            self.scene_biblio.values().map(Self::summary_of).collect();
        summaries.sort_by(|a, b| a.id.cmp(&b.id));
        summaries
    }

    /// Queries scene summary metadata.
    ///
    /// Returns `None` without response if caller lacks permission under non-done status.
    pub fn query_scene(&self, caller_account: u64, scene_id: &str) -> Option<SceneSummary> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        self.scene_summary(scene_id)
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
    ///
    /// Kept for callers that do not have a display name; use
    /// [`Room::create_scene_named`] to write `std::name` atomically at creation.
    pub fn create_scene(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        inherit_juices_from: Option<&str>,
    ) -> Result<()> {
        self.create_scene_named(caller_account, scene_id, inherit_juices_from, None)
    }

    /// Creates a new scene and writes its name to `std::name` in the same step.
    ///
    /// When the scene already exists the name is updated when provided, so a
    /// retried `CreateScene` with a name stays idempotent instead of clobbering
    /// the scene state. `std::name` is not inherited from the juice source.
    pub fn create_scene_named(
        &mut self,
        _caller_account: u64,
        scene_id: &str,
        inherit_juices_from: Option<&str>,
        name: Option<&str>,
    ) -> Result<()> {
        if let Some(existing) = self.scene_biblio.get_mut(scene_id) {
            if let Some(name) = name {
                existing
                    .state
                    .update(KEY_STD_NAME.to_string(), Tin::from_string(name), true)?;
            }
            return Ok(());
        }

        let mut new_scene = Scene::new(scene_id, self.chunk_exponent);

        // Inherit juices (e.g. background image and BGM) if requested. The
        // scene name is not a juice: a derived scene must not inherit the
        // source scene's display name.
        if let Some(source) = inherit_juices_from.and_then(|id| self.scene_biblio.get(id)) {
            for (k, v) in source.state.as_map() {
                if k.starts_with("std::") && k != KEY_STD_ADMINS && k != KEY_STD_NAME {
                    let _ = new_scene.state.update(k.clone(), v.clone(), true);
                }
            }
        }

        if let Some(name) = name {
            new_scene
                .state
                .update(KEY_STD_NAME.to_string(), Tin::from_string(name), true)?;
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
    ///
    /// The destination scene is created implicitly when missing. Membership is
    /// stored on [`Scene::present_characters`] (persisted through `SceneData`),
    /// so the room itself is the source of truth; no `character::<id>::scene`
    /// mirror is needed.
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

        self.assign_character_to_scene(character_id, to_scene_id)?;
        Ok(())
    }

    /// Places a character in `scene_id`, removing it from every other scene.
    ///
    /// Returns the affected scene IDs sorted ascending (source scenes that lost
    /// the character plus the destination). Used by character creation and
    /// movement so callers can broadcast the corresponding `SceneInfo` frames.
    pub fn assign_character_to_scene(
        &mut self,
        character_id: u64,
        scene_id: &str,
    ) -> Result<Vec<String>> {
        if !self.scene_biblio.contains_key(scene_id) {
            return Err(BanquetteError::SceneNotFound(scene_id.to_string()));
        }

        let mut affected = Vec::new();
        for (sid, scene) in &mut self.scene_biblio {
            if sid == scene_id {
                continue;
            }
            if scene.present_characters.remove(&character_id) {
                affected.push(sid.clone());
            }
        }

        if let Some(target) = self.scene_biblio.get_mut(scene_id) {
            target.present_characters.insert(character_id);
        }
        affected.push(scene_id.to_string());
        affected.sort();
        affected.dedup();
        Ok(affected)
    }

    /// Removes a character from every scene and returns the affected scene IDs.
    ///
    /// Called when a character is deleted so no scene keeps a stale member.
    pub fn remove_character_from_scenes(&mut self, character_id: u64) -> Vec<String> {
        let mut affected = Vec::new();
        for (sid, scene) in &mut self.scene_biblio {
            if scene.present_characters.remove(&character_id) {
                affected.push(sid.clone());
            }
        }
        affected.sort();
        affected
    }

    /// Removes the piece state (`piece::<id>::placed` / `piece::<id>::pos`) of a
    /// character from every scene.
    ///
    /// Called on deletion so no client keeps an interactive stale piece. The
    /// removal is an internal cleanup and bypasses the caller check (it cannot
    /// touch `std::admins`). Returns the `(scene_id, key)` pairs that were
    /// actually present, sorted, so the server can announce `StateDeleted`.
    pub fn clear_character_piece_state(&mut self, character_id: u64) -> Vec<(String, String)> {
        let keys = [
            format!("piece::{character_id}::placed"),
            format!("piece::{character_id}::pos"),
        ];
        let mut removed = Vec::new();
        for (sid, scene) in &mut self.scene_biblio {
            for key in &keys {
                if let Ok(Some(_)) = scene.state.delete(key, true) {
                    removed.push((sid.clone(), key.clone()));
                }
            }
        }
        removed.sort();
        removed
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

    /// Persists the whole room state to cellar and advances the in-memory head pointer.
    ///
    /// The store is authorized by the cellar service token carried by the
    /// [`CellarSink`] (or by a real user session when no token is configured);
    /// internal writes never impersonate a synthetic session, so
    /// [`INTERNAL_WRITE_SESSION_ID`] is sent. The reference label is
    /// [`alhalte_cellar::SYSTEM_OWNER`]: the room administrator is not the owner
    /// of internal room writes, and `owner` is not an authorization boundary.
    ///
    /// After a successful store the previous head tin is unregistered on a
    /// best-effort basis so cellar GC can reclaim it. The current head is never
    /// unregistered, and retention failures are logged but do not fail the store.
    ///
    /// Returns the tin ID of the stored `RoomData`.
    pub async fn persist(&mut self, cellar: &dyn CellarSink) -> Result<TinId> {
        let room_tin = self.to_tin()?;
        let (wire_bytes, tin_id) = room_tin
            .encode_plaintext(Compression::Zstd)
            .map_err(BanquetteError::Tin)?;

        cellar
            .store_tin(
                alhalte_cellar::SYSTEM_OWNER,
                INTERNAL_WRITE_SESSION_ID,
                &wire_bytes,
                &tin_id,
                None,
            )
            .await?;

        let previous = self.head_tin.replace(tin_id);
        if let Some(previous) = previous
            && previous != tin_id
        {
            match cellar
                .unregister_tin(
                    alhalte_cellar::SYSTEM_OWNER,
                    INTERNAL_WRITE_SESSION_ID,
                    &previous,
                )
                .await
            {
                Ok(true) => tracing::debug!(
                    "unregistered superseded room head tin {}",
                    alhalte_cellar::format_tin_id(&previous)
                ),
                Ok(false) => tracing::debug!(
                    "superseded room head tin {} already had no reference",
                    alhalte_cellar::format_tin_id(&previous)
                ),
                Err(e) => tracing::warn!(
                    "failed to unregister superseded room head tin {}: {e}",
                    alhalte_cellar::format_tin_id(&previous)
                ),
            }
        }

        Ok(tin_id)
    }

    /// Advances one heartbeat cycle.
    ///
    /// - Persists the whole room state and reports the stored `RoomData` tin ID
    ///   in [`HeartbeatResult::persisted_tin`]. A failed store is logged and
    ///   reported as `None` so the process keeps running and retries next cycle.
    /// - Checks client activity. If no client activity occurred during the cycle, increments `missed_heartbeats`.
    /// - If 3 consecutive missed heartbeats are accumulated, returns [`HeartbeatOutcome::ShouldExit`].
    pub async fn step_heartbeat(&mut self, cellar: &dyn CellarSink) -> Result<HeartbeatResult> {
        let persisted_tin = match self.persist(cellar).await {
            Ok(tin_id) => Some(tin_id),
            Err(e) => {
                tracing::error!("Failed to persist room state to cellar: {e}");
                None
            }
        };

        // Evaluate activity
        let had_activity = self.client_activity_in_cycle;
        self.client_activity_in_cycle = false;

        // Also check if any clients are currently connected
        let any_connected_clients = self
            .scene_biblio
            .values()
            .any(|s| !s.active_clients.is_empty());

        let outcome = if had_activity || any_connected_clients {
            self.missed_heartbeats = 0;
            HeartbeatOutcome::Continue
        } else {
            self.missed_heartbeats = self.missed_heartbeats.saturating_add(1);
            if self.missed_heartbeats >= 3 {
                HeartbeatOutcome::ShouldExit
            } else {
                HeartbeatOutcome::Continue
            }
        };

        Ok(HeartbeatResult {
            outcome,
            persisted_tin,
        })
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

        if !scene_biblio.contains_key("/") {
            return Err(BanquetteError::MissingRootScene);
        }

        Ok(Self {
            room_id: data.room_id,
            done: data.done,
            chunk_exponent: data.chunk_exponent,
            scene_biblio,
            character_biblio: data.character_biblio,
            missed_heartbeats: 0,
            client_activity_in_cycle: true,
            head_tin: None,
        })
    }

    /// Reconstructs a [`Room`] from a persisted `RoomData` tin and records its tin ID as the head.
    ///
    /// The head is kept so that the first successful re-persist can unregister
    /// the superseded head from cellar (retention).
    pub fn from_tin_with_head(tin: &Tin, head_tin: TinId) -> Result<Self> {
        let mut room = Self::from_tin(tin)?;
        room.head_tin = Some(head_tin);
        Ok(room)
    }

    /// Downloads, decodes, and validates a persisted room from cellar.
    ///
    /// This is the restore path used when `maitre-d` starts a banquette for a
    /// room that already has a persisted head. It never falls back to an empty
    /// room: a missing tin, a corrupted payload, a content hash that does not
    /// match the head pointer, a missing root scene, or a room ID mismatch all
    /// return an error so the caller can fail the spawn loudly.
    pub async fn load_from_cellar(
        cellar: &dyn CellarSink,
        tin_id: &TinId,
        expected_room_id: u64,
    ) -> Result<Self> {
        let bytes = cellar.download_tin(tin_id, None).await.map_err(|e| {
            BanquetteError::Cellar(format!(
                "failed to download room tin {}: {e}",
                alhalte_cellar::format_tin_id(tin_id)
            ))
        })?;

        let (tin, _opener, decoded_tin_id) = Tin::decode(&bytes, None).map_err(|e| {
            BanquetteError::Cellar(format!(
                "failed to decode room tin {}: {e}",
                alhalte_cellar::format_tin_id(tin_id)
            ))
        })?;

        if decoded_tin_id != *tin_id {
            return Err(BanquetteError::TinIdMismatch {
                expected: alhalte_cellar::format_tin_id(tin_id),
                actual: alhalte_cellar::format_tin_id(&decoded_tin_id),
            });
        }

        let room = Self::from_tin_with_head(&tin, *tin_id)?;
        if room.room_id != expected_room_id {
            return Err(BanquetteError::RoomIdMismatch {
                expected: expected_room_id,
                actual: room.room_id,
            });
        }

        Ok(room)
    }

    /// Exports the room data and associated log chunks for permitted scenes.
    pub async fn export_snapshot(
        &self,
        cellar: &dyn CellarSink,
        caller_account: Option<u64>,
    ) -> Result<(RoomData, Vec<(TinId, LogChunk)>)> {
        let mut scenes_data = HashMap::new();
        let mut all_chunks = Vec::new();

        for (sid, scene) in &self.scene_biblio {
            let can_access = match caller_account {
                Some(acc) => self.can_access_scene(acc, sid),
                None => self.done || sid == "/",
            };
            if !can_access {
                continue;
            }

            let mut biblio = scene.log.biblio.clone();

            // 1. Read all sealed chunks
            for (idx, &tid) in scene.log.biblio.chunks.iter().enumerate() {
                let entries = scene
                    .log
                    .read_chunk(idx as u32, cellar, caller_account)
                    .await?;
                all_chunks.push((tid, LogChunk::new(entries)));
            }

            // 2. If there are active unsealed entries, seal them for the export snapshot
            if !scene.log.active_chunk.is_empty() {
                let active_chunk = LogChunk::new(scene.log.active_chunk.clone());
                let chunk_tin = active_chunk.to_tin()?;
                let (_, active_tid) = chunk_tin
                    .encode_plaintext(Compression::Zstd)
                    .map_err(BanquetteError::Tin)?;
                biblio.chunks.push(active_tid);
                all_chunks.push((active_tid, active_chunk));
            }

            scenes_data.insert(
                sid.clone(),
                SceneData {
                    id: scene.id.clone(),
                    states: scene.state.as_map().clone(),
                    biblio,
                    present_characters: scene.present_characters.iter().copied().collect(),
                },
            );
        }

        let room_data = RoomData {
            room_id: self.room_id,
            done: self.done,
            chunk_exponent: self.chunk_exponent,
            character_biblio: self.character_biblio.clone(),
            scenes: scenes_data,
        };

        Ok((room_data, all_chunks))
    }
}
