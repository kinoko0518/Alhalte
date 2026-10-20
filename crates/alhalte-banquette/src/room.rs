//! Room domain model, permissions, scene orchestration, and lifecycle management.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use tin::{Compression, Tin, TinId};

use crate::cellar_sink::{CellarSink, INTERNAL_WRITE_SESSION_ID};
use crate::character::{Character, CharacterBiblio, get_character_owner};
use crate::error::{BanquetteError, Result};
use crate::labels::{KEY_STD_NAME, LABEL_ALHALTE_BANQUETTE};
use crate::log::{LogBiblio, LogChunk, LogOperationEvent, SceneLog};
use crate::scene::{Scene, SceneData};
use crate::state::SceneState;

/// Access level an account holds inside one room.
///
/// The level is stored per room ([`RoomData::permissions`]) with
/// [`RoomData::default_permission`] as the fallback for accounts without an
/// entry. It is never expressed through `"/"` scene state and never duplicated
/// in `maitre-d`'s database: the persisted [`RoomData`] is the source of truth.
///
/// spec: alhalte-banquette/permissions 0001 four permission levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RoomPermissionLevel {
    /// The account is refused entry to the room.
    Denied,
    /// The account receives the log and state of every scene but may not write.
    Viewer,
    /// The account may read and write the scenes an administrator moved it into,
    /// plus the root scene `"/"`.
    Participant,
    /// The account may read and write every scene and change the permission table.
    Admin,
}

impl RoomPermissionLevel {
    /// Returns whether this level may mutate room data at all.
    ///
    /// [`RoomPermissionLevel::Viewer`] is receive-only, so every mutate frame is
    /// rejected for it; [`RoomPermissionLevel::Participant`] and
    /// [`RoomPermissionLevel::Admin`] write, subject to scene authorization.
    pub const fn can_write(self) -> bool {
        matches!(self, Self::Participant | Self::Admin)
    }
}

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
///
/// The wire schema replaced the former `done: bool` with
/// [`RoomData::default_permission`], [`RoomData::permissions`], and
/// [`RoomData::accessible_scenes`]. Compatibility is intentionally cut
/// (Issue #199): a `RoomData` written by the old schema either fails to decode
/// or fails [`Room::validate`], and the caller refuses to start instead of
/// opening an empty room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomData {
    /// 64-bit room identifier.
    pub room_id: u64,
    /// Permission applied to accounts without an entry in `permissions`.
    pub default_permission: RoomPermissionLevel,
    /// Per-account permission overrides.
    ///
    /// An explicit entry always wins over `default_permission`, including an
    /// explicit [`RoomPermissionLevel::Denied`] for a room whose default is more
    /// permissive.
    pub permissions: BTreeMap<u64, RoomPermissionLevel>,
    /// Scenes an account may participate in, keyed by account ID.
    ///
    /// Only accounts that can write (see [`RoomPermissionLevel::can_write`]) may
    /// hold entries, the scenes must exist, and the lists are sorted and
    /// deduplicated. Both invariants are enforced by [`Room::validate`].
    pub accessible_scenes: BTreeMap<u64, Vec<String>>,
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

/// Cellar alias name holding `room_id`'s persisted `RoomData` head.
///
/// The room head lives in cellar, not in `alhalte-maitre-d`: the alias
/// `room/<room_id>/head` points at the tin of the last stored [`RoomData`], so a
/// freshly started banquette can restore itself without `--room-tin` and without
/// asking maitre-d (Issue #192). The room ID is written in canonical 16-hex form.
pub fn room_head_alias(room_id: u64) -> String {
    format!("room/{room_id:016x}/head")
}

/// The core session room process managing scenes, characters, and client presence.
#[derive(Debug)]
pub struct Room {
    /// 64-bit room identifier.
    pub room_id: u64,
    /// Permission applied to accounts without an entry in `permissions`.
    pub default_permission: RoomPermissionLevel,
    /// Per-account permission overrides.
    pub permissions: BTreeMap<u64, RoomPermissionLevel>,
    /// Scenes each account may participate in.
    pub accessible_scenes: BTreeMap<u64, Vec<String>>,
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
    ///
    /// The room starts closed: [`RoomPermissionLevel::Denied`] is the default
    /// permission, so only accounts an administrator admits can do anything. The
    /// creator becomes the initial [`RoomPermissionLevel::Admin`] and gets `"/"`
    /// as an accessible scene.
    pub fn new(room_id: u64, creator_account: u64, chunk_exponent: u8) -> Result<Self> {
        let mut room = Self {
            room_id,
            default_permission: RoomPermissionLevel::Denied,
            permissions: BTreeMap::new(),
            accessible_scenes: BTreeMap::new(),
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
        )?;
        room.scene_biblio.insert("/".to_string(), root_scene);

        room.permissions
            .insert(creator_account, RoomPermissionLevel::Admin);
        room.grant_accessible_scene(creator_account, "/");
        room.validate()?;
        Ok(room)
    }

    // -----------------------------------------------------------------------
    // Permissions
    // -----------------------------------------------------------------------

    /// Returns the effective permission of `account_id` in this room.
    pub fn get_account_permission(&self, account_id: u64) -> RoomPermissionLevel {
        self.permissions
            .get(&account_id)
            .copied()
            .unwrap_or(self.default_permission)
    }

    /// Checks if a given account ID is an administrator of the room.
    pub fn is_admin(&self, account_id: u64) -> bool {
        self.get_account_permission(account_id) == RoomPermissionLevel::Admin
    }

    /// Returns the account IDs with an explicit [`RoomPermissionLevel::Admin`] entry.
    ///
    /// Accounts that are administrators only through
    /// [`Room::default_permission`] are not listed.
    pub fn admin_accounts(&self) -> Vec<u64> {
        self.permissions
            .iter()
            .filter(|(_, level)| **level == RoomPermissionLevel::Admin)
            .map(|(account_id, _)| *account_id)
            .collect()
    }

    /// Returns the current `(default_permission, permissions)` pair.
    ///
    /// This is the payload of `ServerFrame::PermissionsSnapshot` /
    /// `ServerFrame::PermissionsUpdated`.
    pub fn permission_snapshot(&self) -> (RoomPermissionLevel, BTreeMap<u64, RoomPermissionLevel>) {
        (self.default_permission, self.permissions.clone())
    }

    /// Sets the permission of a single account. Requires an administrator caller.
    ///
    /// Demoting the last administrator is rejected with
    /// [`BanquetteError::LastAdminDemotion`]. The change is rolled back when the
    /// resulting room would fail [`Room::validate`].
    pub fn set_member_permission(
        &mut self,
        caller_account: u64,
        account_id: u64,
        permission: RoomPermissionLevel,
    ) -> Result<()> {
        self.authorize_admin(caller_account)?;
        if permission != RoomPermissionLevel::Admin
            && self.is_admin(account_id)
            && !self.has_other_admin(account_id)
        {
            return Err(BanquetteError::LastAdminDemotion);
        }

        let previous_permissions = self.permissions.clone();
        let previous_accessible = self.accessible_scenes.clone();

        self.permissions.insert(account_id, permission);
        if permission.can_write() {
            self.grant_accessible_scene(account_id, "/");
        }
        self.normalize_accessible_scenes();

        if let Err(error) = self.validate() {
            self.permissions = previous_permissions;
            self.accessible_scenes = previous_accessible;
            return Err(error);
        }

        tracing::info!(
            "room {} set account {account_id} permission to {permission:?}",
            self.room_id
        );
        Ok(())
    }

    /// Sets the permission applied to accounts without an explicit entry.
    ///
    /// Requires an administrator caller and refuses to demote the last
    /// administrator, exactly like [`Room::set_member_permission`].
    pub fn set_default_permission(
        &mut self,
        caller_account: u64,
        permission: RoomPermissionLevel,
    ) -> Result<()> {
        self.authorize_admin(caller_account)?;
        if permission != RoomPermissionLevel::Admin
            && self.default_permission == RoomPermissionLevel::Admin
            && self.admin_accounts().is_empty()
        {
            return Err(BanquetteError::LastAdminDemotion);
        }

        let previous_default = self.default_permission;
        let previous_accessible = self.accessible_scenes.clone();

        self.default_permission = permission;
        self.normalize_accessible_scenes();

        if let Err(error) = self.validate() {
            self.default_permission = previous_default;
            self.accessible_scenes = previous_accessible;
            return Err(error);
        }

        tracing::info!(
            "room {} set default permission to {permission:?}",
            self.room_id
        );
        Ok(())
    }

    /// Validates the room invariants.
    ///
    /// Called after loading a persisted room and after every permission change:
    ///
    /// - the room keeps at least one administrator (explicitly or through the
    ///   default permission),
    /// - `accessible_scenes` only lists scenes that exist,
    /// - `accessible_scenes` only lists accounts that can write, so a
    ///   [`RoomPermissionLevel::Viewer`] or [`RoomPermissionLevel::Denied`]
    ///   account never carries scene grants.
    pub fn validate(&self) -> Result<()> {
        if !self.has_admin() {
            return Err(BanquetteError::InvalidRoomData(
                "a room must keep at least one administrator".to_string(),
            ));
        }

        for (account_id, scenes) in &self.accessible_scenes {
            let level = self.get_account_permission(*account_id);
            if !level.can_write() {
                return Err(BanquetteError::InvalidRoomData(format!(
                    "account {account_id} holds accessible scenes at permission {level:?}"
                )));
            }
            for scene_id in scenes {
                if !self.scene_biblio.contains_key(scene_id) {
                    return Err(BanquetteError::InvalidRoomData(format!(
                        "account {account_id} references unknown scene '{scene_id}'"
                    )));
                }
            }
        }

        Ok(())
    }

    /// Checks whether an account may receive the log / state of a scene.
    ///
    /// - `Denied`: never.
    /// - `Viewer`: every scene.
    /// - `Participant`: `"/"` and the scenes granted by an administrator.
    /// - `Admin`: every scene.
    pub fn can_access_scene(&self, account_id: u64, scene_id: &str) -> bool {
        if !self.scene_biblio.contains_key(scene_id) {
            return false;
        }
        match self.get_account_permission(account_id) {
            RoomPermissionLevel::Denied => false,
            RoomPermissionLevel::Viewer | RoomPermissionLevel::Admin => true,
            RoomPermissionLevel::Participant => {
                scene_id == "/" || self.scene_is_accessible(account_id, scene_id)
            }
        }
    }

    /// Checks whether an account may mutate a scene (log / state writes).
    ///
    /// Same scene set as [`Room::can_access_scene`], but `Viewer` and `Denied`
    /// never write.
    pub fn can_send_to_scene(&self, account_id: u64, scene_id: &str) -> bool {
        if !self.scene_biblio.contains_key(scene_id) {
            return false;
        }
        match self.get_account_permission(account_id) {
            RoomPermissionLevel::Denied | RoomPermissionLevel::Viewer => false,
            RoomPermissionLevel::Admin => true,
            RoomPermissionLevel::Participant => {
                scene_id == "/" || self.scene_is_accessible(account_id, scene_id)
            }
        }
    }

    /// Returns the scenes explicitly granted to `account_id`.
    pub fn accessible_scenes_of(&self, account_id: u64) -> &[String] {
        self.accessible_scenes
            .get(&account_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Returns whether the room keeps at least one administrator.
    fn has_admin(&self) -> bool {
        self.default_permission == RoomPermissionLevel::Admin
            || self
                .permissions
                .values()
                .any(|level| *level == RoomPermissionLevel::Admin)
    }

    /// Returns whether any account other than `account_id` is an administrator.
    ///
    /// An `Admin` default permission keeps every unlisted account an
    /// administrator, so demoting one explicit entry cannot empty the set.
    fn has_other_admin(&self, account_id: u64) -> bool {
        if self.default_permission == RoomPermissionLevel::Admin {
            return true;
        }
        self.permissions
            .iter()
            .any(|(other, level)| *other != account_id && *level == RoomPermissionLevel::Admin)
    }

    /// Returns whether `scene_id` was explicitly granted to `account_id`.
    fn scene_is_accessible(&self, account_id: u64, scene_id: &str) -> bool {
        self.accessible_scenes
            .get(&account_id)
            .is_some_and(|scenes| scenes.iter().any(|scene| scene == scene_id))
    }

    /// Grants `scene_id` to `account_id`, keeping the list sorted and unique.
    ///
    /// Grants are only recorded for accounts that can write and for scenes that
    /// exist, so [`Room::validate`] keeps holding without extra checks.
    fn grant_accessible_scene(&mut self, account_id: u64, scene_id: &str) {
        if !self.get_account_permission(account_id).can_write()
            || !self.scene_biblio.contains_key(scene_id)
        {
            return;
        }
        let scenes = self.accessible_scenes.entry(account_id).or_default();
        if let Err(index) = scenes.binary_search_by(|probe| probe.as_str().cmp(scene_id)) {
            scenes.insert(index, scene_id.to_string());
        }
    }

    /// Drops scene grants that are no longer valid.
    ///
    /// Grants are removed when the account lost write access, when the scene was
    /// deleted, and empty lists are dropped entirely.
    fn normalize_accessible_scenes(&mut self) {
        let default_permission = self.default_permission;
        let permissions = &self.permissions;
        let scene_biblio = &self.scene_biblio;
        self.accessible_scenes.retain(|account_id, scenes| {
            let level = permissions
                .get(account_id)
                .copied()
                .unwrap_or(default_permission);
            if !level.can_write() {
                return false;
            }
            scenes.retain(|scene_id| scene_biblio.contains_key(scene_id));
            scenes.sort();
            scenes.dedup();
            !scenes.is_empty()
        });
    }

    /// Requires `caller_account` to be an administrator.
    fn authorize_admin(&self, caller_account: u64) -> Result<()> {
        if !self.is_admin(caller_account) {
            return Err(BanquetteError::PermissionDenied);
        }
        Ok(())
    }

    /// Requires `caller_account` to be allowed to write to `scene_id`.
    fn authorize_send(&self, caller_account: u64, scene_id: &str) -> Result<()> {
        if !self.scene_biblio.contains_key(scene_id) {
            return Err(BanquetteError::SceneNotFound(scene_id.to_string()));
        }
        if !self.can_send_to_scene(caller_account, scene_id) {
            return Err(BanquetteError::PermissionDenied);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Scene queries
    // -----------------------------------------------------------------------

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
    /// Returns `None` without response if caller lacks [`Room::can_access_scene`].
    pub fn query_scene(&self, caller_account: u64, scene_id: &str) -> Option<SceneSummary> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        self.scene_summary(scene_id)
    }

    /// Queries scene state.
    ///
    /// Returns `None` without response if caller lacks [`Room::can_access_scene`].
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
    /// Returns `None` without response if caller lacks [`Room::can_access_scene`].
    pub fn query_log_biblio(&self, caller_account: u64, scene_id: &str) -> Option<&LogBiblio> {
        if !self.can_access_scene(caller_account, scene_id) {
            return None;
        }

        let scene = self.scene_biblio.get(scene_id)?;
        Some(&scene.log.biblio)
    }

    /// Queries a chunk of log entries.
    ///
    /// Returns `None` without response if caller lacks [`Room::can_access_scene`].
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

    // -----------------------------------------------------------------------
    // Scene mutations
    // -----------------------------------------------------------------------

    /// Creates a new scene in the room.
    ///
    /// Kept for callers that do not have a display name; use
    /// [`Room::create_scene_named`] to write `std::name` atomically at creation.
    /// Scene structure is an administrator operation.
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
    /// Requires an administrator caller. When the scene already exists the name
    /// is updated when provided, so a retried `CreateScene` with a name stays
    /// idempotent instead of clobbering the scene state. `std::name` is not
    /// inherited from the juice source.
    pub fn create_scene_named(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        inherit_juices_from: Option<&str>,
        name: Option<&str>,
    ) -> Result<()> {
        self.authorize_admin(caller_account)?;

        if let Some(existing) = self.scene_biblio.get_mut(scene_id) {
            if let Some(name) = name {
                existing
                    .state
                    .update(KEY_STD_NAME.to_string(), Tin::from_string(name))?;
            }
            return Ok(());
        }

        let mut new_scene = Scene::new(scene_id, self.chunk_exponent);

        // Inherit juices (e.g. background image and BGM) if requested. The
        // scene name is not a juice: a derived scene must not inherit the
        // source scene's display name.
        if let Some(source) = inherit_juices_from.and_then(|id| self.scene_biblio.get(id)) {
            for (k, v) in source.state.as_map() {
                if k.starts_with("std::") && k != KEY_STD_NAME {
                    let _ = new_scene.state.update(k.clone(), v.clone());
                }
            }
        }

        if let Some(name) = name {
            new_scene
                .state
                .update(KEY_STD_NAME.to_string(), Tin::from_string(name))?;
        }

        self.scene_biblio.insert(scene_id.to_string(), new_scene);
        Ok(())
    }

    /// Deletes a scene.
    ///
    /// Requires an administrator caller. The root scene `"/"` cannot be deleted,
    /// and scene grants that referenced the removed scene are dropped.
    pub fn delete_scene(&mut self, caller_account: u64, scene_id: &str) -> Result<()> {
        self.authorize_admin(caller_account)?;
        if scene_id == "/" {
            return Err(BanquetteError::PermissionDenied);
        }

        self.scene_biblio
            .remove(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;
        self.normalize_accessible_scenes();
        Ok(())
    }

    /// Updates scene state.
    ///
    /// Requires [`Room::can_send_to_scene`] on the target scene, so a
    /// `Participant` writes the scenes it was moved into and the root scene,
    /// while a `Viewer` never writes.
    pub fn update_scene_state(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        key: String,
        value: Tin,
    ) -> Result<Option<Tin>> {
        self.authorize_send(caller_account, scene_id)?;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene.state.update(key, value)
    }

    /// Deletes a key from scene state.
    ///
    /// Requires [`Room::can_send_to_scene`] on the target scene.
    pub fn delete_scene_state(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        key: &str,
    ) -> Result<Option<Tin>> {
        self.authorize_send(caller_account, scene_id)?;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene.state.delete(key)
    }

    // -----------------------------------------------------------------------
    // Log mutations
    // -----------------------------------------------------------------------

    /// Appends a log entry to a scene.
    ///
    /// Requires [`Room::can_send_to_scene`] on the target scene.
    pub async fn append_log(
        &mut self,
        caller_account: u64,
        session_id: u64,
        scene_id: &str,
        entry: Option<Tin>,
        cellar: &dyn CellarSink,
    ) -> Result<LogOperationEvent> {
        self.authorize_send(caller_account, scene_id)?;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene
            .log
            .append(entry, cellar, caller_account, session_id)
            .await
    }

    /// Edits an existing log entry before the head.
    ///
    /// Requires [`Room::can_send_to_scene`] on the target scene.
    ///
    /// The parameter list mirrors the wire frame and the append/delete paths;
    /// grouping it into a struct would obscure the one-to-one mapping.
    #[allow(clippy::too_many_arguments)]
    pub async fn edit_log(
        &mut self,
        caller_account: u64,
        session_id: u64,
        scene_id: &str,
        chunk_index: u32,
        position: u32,
        entry: Tin,
        cellar: &dyn CellarSink,
    ) -> Result<LogOperationEvent> {
        self.authorize_send(caller_account, scene_id)?;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene
            .log
            .edit(
                chunk_index,
                position,
                entry,
                cellar,
                caller_account,
                session_id,
            )
            .await
    }

    /// Deletes an existing log entry before the head (tombstone).
    ///
    /// Requires [`Room::can_send_to_scene`] on the target scene.
    pub async fn delete_log(
        &mut self,
        caller_account: u64,
        session_id: u64,
        scene_id: &str,
        chunk_index: u32,
        position: u32,
        cellar: &dyn CellarSink,
    ) -> Result<LogOperationEvent> {
        self.authorize_send(caller_account, scene_id)?;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene
            .log
            .delete(chunk_index, position, cellar, caller_account, session_id)
            .await
    }

    /// Rewinds the log head to an earlier position.
    ///
    /// Requires [`Room::can_send_to_scene`] on the target scene.
    pub fn rewind_log(
        &mut self,
        caller_account: u64,
        scene_id: &str,
        chunk_index: u32,
        position: u32,
    ) -> Result<LogOperationEvent> {
        self.authorize_send(caller_account, scene_id)?;

        let scene = self
            .scene_biblio
            .get_mut(scene_id)
            .ok_or_else(|| BanquetteError::SceneNotFound(scene_id.to_string()))?;

        scene.log.rewind(chunk_index, position)
    }

    // -----------------------------------------------------------------------
    // Characters
    // -----------------------------------------------------------------------

    /// Allocates a character owned by `creator_account`.
    ///
    /// Requires write access (`Participant` or `Admin`), so a `Viewer` cannot
    /// create characters. Scene placement is the caller's responsibility (see
    /// [`Room::assign_character_to_scene`]).
    pub fn create_character(&mut self, creator_account: u64, data: Character) -> Result<u64> {
        if !self.get_account_permission(creator_account).can_write() {
            return Err(BanquetteError::PermissionDenied);
        }
        Ok(self
            .character_biblio
            .create_character(creator_account, data))
    }

    /// Updates a character. Requires write access and ownership or `Admin`.
    pub fn update_character(
        &mut self,
        caller_account: u64,
        character_id: u64,
        data: Character,
    ) -> Result<()> {
        let level = self.get_account_permission(caller_account);
        if !level.can_write() {
            return Err(BanquetteError::PermissionDenied);
        }
        self.character_biblio.update(
            character_id,
            caller_account,
            level == RoomPermissionLevel::Admin,
            data,
        )
    }

    /// Deletes a character. Requires write access and ownership or `Admin`.
    ///
    /// The caller is responsible for removing the scene membership and piece
    /// state afterwards (see [`Room::remove_character_from_scenes`] and
    /// [`Room::clear_character_piece_state`]).
    pub fn delete_character(
        &mut self,
        caller_account: u64,
        character_id: u64,
    ) -> Result<Character> {
        let level = self.get_account_permission(caller_account);
        if !level.can_write() {
            return Err(BanquetteError::PermissionDenied);
        }
        self.character_biblio.delete(
            character_id,
            caller_account,
            level == RoomPermissionLevel::Admin,
        )
    }

    /// Moves a character to a target scene.
    ///
    /// Only an administrator may move a character. A participant cannot move
    /// even its own character: being moved is how an administrator grants the
    /// participant access to a scene. When the mover is an administrator, the
    /// destination scene is added to the character owner's
    /// [`Room::accessible_scenes`] so the participant can then read and write
    /// it.
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
        self.authorize_admin(caller_account)?;

        let owner_account = self
            .character_biblio
            .get(character_id)
            .ok_or(BanquetteError::CharacterNotFound(character_id))
            .map(get_character_owner)?;

        if !self.scene_biblio.contains_key(to_scene_id) {
            self.create_scene(caller_account, to_scene_id, Some("/"))?;
        }

        self.assign_character_to_scene(character_id, to_scene_id)?;

        if let Some(owner_account) = owner_account {
            self.grant_accessible_scene(owner_account, to_scene_id);
        }
        Ok(())
    }

    /// Places a character in `scene_id`, removing it from every other scene.
    ///
    /// Internal helper without a permission check: callers authorize the move
    /// first ([`Room::move_character`] or `CreateCharacter`).
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
    /// removal is an internal cleanup that bypasses the per-scene write check
    /// (room permissions live outside scene state, so no reserved key is
    /// involved). Returns the `(scene_id, key)` pairs that were actually
    /// present, sorted, so the server can announce `StateDeleted`.
    pub fn clear_character_piece_state(&mut self, character_id: u64) -> Vec<(String, String)> {
        let keys = [
            format!("piece::{character_id}::placed"),
            format!("piece::{character_id}::pos"),
        ];
        let mut removed = Vec::new();
        for (sid, scene) in &mut self.scene_biblio {
            for key in &keys {
                if let Ok(Some(_)) = scene.state.delete(key) {
                    removed.push((sid.clone(), key.clone()));
                }
            }
        }
        removed.sort();
        removed
    }

    // -----------------------------------------------------------------------
    // Client presence and lifecycle
    // -----------------------------------------------------------------------

    /// Loads `scene_id` from cellar when it is currently unloaded.
    ///
    /// A scene keeps only its biblio and membership in `RoomData`; its memory is
    /// dropped by [`Scene::persist_and_unload`] and rebuilt by
    /// [`Scene::reload_if_needed`]. Any membership or log write performed while
    /// the scene is unloaded therefore lives in memory that the next reload
    /// overwrites, so a caller that mutates a scene it did not enter must load it
    /// first (Issue #240: a character moved from the character list into a scene
    /// that is not open became unaffiliated as soon as the destination was
    /// entered).
    ///
    /// A missing scene or an already loaded scene is a no-op: whether a missing
    /// scene is an error is the caller's decision (`MoveCharacter` creates it
    /// instead). `caller_account` is the owner used for the cellar download, so
    /// the reload keeps the same authorization as
    /// [`Room::client_enter_scene`].
    pub async fn ensure_scene_loaded(
        &mut self,
        scene_id: &str,
        caller_account: u64,
        cellar: &dyn CellarSink,
    ) -> Result<()> {
        let Some(scene) = self.scene_biblio.get_mut(scene_id) else {
            return Ok(());
        };
        if scene.is_loaded {
            return Ok(());
        }
        scene.reload_if_needed(cellar, Some(caller_account)).await
    }

    /// Handles client entering a scene.
    ///
    /// If the scene was unloaded, restores it from cellar. Entry itself is
    /// authorized by the caller (`can_access_scene`), not here.
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

    /// Persists the whole room state to cellar, moves the room head alias, and
    /// advances the in-memory head pointer.
    ///
    /// The store is authorized by the cellar service token carried by the
    /// [`CellarSink`] (or by a real user session when no token is configured);
    /// internal writes never impersonate a synthetic session, so
    /// [`INTERNAL_WRITE_SESSION_ID`] is sent. The reference label is
    /// [`alhalte_cellar::SYSTEM_OWNER`]: the room administrator is not the owner
    /// of internal room writes, and `owner` is not an authorization boundary.
    ///
    /// The `room/<room_id>/head` alias is moved to the new tin **before** the
    /// previous head is unregistered. The alias is the durable restore point, so
    /// a failed alias write returns an error, keeps the previous alias and head
    /// valid, and lets the heartbeat retry next cycle.
    ///
    /// Retention of the superseded head is best effort: the previous head is
    /// unregistered after the alias moved so cellar GC can reclaim it, and a
    /// retention failure is logged without failing the store. The current head
    /// is never unregistered.
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

        // The alias is the authoritative head pointer, so it moves first.
        cellar
            .set_alias(&room_head_alias(self.room_id), &tin_id)
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

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

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
            default_permission: self.default_permission,
            permissions: self.permissions.clone(),
            accessible_scenes: self.accessible_scenes.clone(),
            chunk_exponent: self.chunk_exponent,
            character_biblio: self.character_biblio.clone(),
            scenes: scenes_data,
        };

        Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &data).map_err(BanquetteError::Tin)
    }

    /// Reconstructs a [`Room`] from a `Tin` with label `alhalte/banquette`.
    ///
    /// A payload written by the pre-#199 schema has no `default_permission` /
    /// `permissions` / `accessible_scenes`, so it either fails to decode
    /// ([`BanquetteError::Tin`]) or fails [`Room::validate`]. The old form is
    /// never converted and never silently accepted as an empty room.
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

        let room = Self {
            room_id: data.room_id,
            default_permission: data.default_permission,
            permissions: data.permissions,
            accessible_scenes: data.accessible_scenes,
            chunk_exponent: data.chunk_exponent,
            scene_biblio,
            character_biblio: data.character_biblio,
            missed_heartbeats: 0,
            client_activity_in_cycle: true,
            head_tin: None,
        };
        room.validate()?;
        Ok(room)
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
    /// room: a missing tin, a corrupted payload, a payload written by the
    /// pre-#199 schema, a content hash that does not match the head pointer, a
    /// missing root scene, a room ID mismatch, or a failed [`Room::validate`] all
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

    /// Restores the room from its `room/<room_id>/head` alias, or starts a new room.
    ///
    /// This is the startup path shared by the standalone binary and maitre-d's
    /// in-process spawner. An unset alias means the room was never persisted, so
    /// a new room is created with `admin_account` as its initial `Admin`.
    ///
    /// A set alias is authoritative: a failed alias read, a missing or corrupt
    /// tin, a content hash that does not match the alias, a room ID mismatch, or
    /// a failed [`Room::validate`] all return an error. Startup fails instead of
    /// silently opening an empty room over a persisted one.
    pub async fn load_or_create(
        cellar: &dyn CellarSink,
        room_id: u64,
        admin_account: u64,
        chunk_exponent: u8,
    ) -> Result<Self> {
        let alias = room_head_alias(room_id);
        let head = cellar.get_alias(&alias).await.map_err(|e| {
            BanquetteError::Cellar(format!(
                "failed to read room head alias {alias} for room {room_id:016x}: {e}"
            ))
        })?;

        let Some(tin_id) = head else {
            return Self::new(room_id, admin_account, chunk_exponent);
        };

        Self::load_from_cellar(cellar, &tin_id, room_id)
            .await
            .map_err(|e| {
                BanquetteError::Cellar(format!(
                    "failed to restore room {room_id:016x} from alias {alias} (tin {}): {e}",
                    alhalte_cellar::format_tin_id(&tin_id)
                ))
            })
    }

    // -----------------------------------------------------------------------
    // Export
    // -----------------------------------------------------------------------

    /// Exports the room data and associated log chunks for permitted scenes.
    ///
    /// Scenes the caller cannot access are dropped from the exported
    /// [`RoomData`] (see [`Room::can_access_scene`]). The permission table itself
    /// is exported as-is: it describes the room, not the caller's view.
    pub async fn export_snapshot(
        &self,
        cellar: &dyn CellarSink,
        caller_account: Option<u64>,
    ) -> Result<(RoomData, Vec<(TinId, LogChunk)>)> {
        let mut scenes_data = HashMap::new();
        let mut all_chunks = Vec::new();

        for (sid, scene) in &self.scene_biblio {
            let can_access = match caller_account {
                Some(account_id) => self.can_access_scene(account_id, sid),
                // An unauthenticated export follows the room default. The root
                // scene is always included so the exported `RoomData` stays
                // structurally valid.
                None => self.default_permission != RoomPermissionLevel::Denied || sid == "/",
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
            default_permission: self.default_permission,
            permissions: self.permissions.clone(),
            accessible_scenes: self.accessible_scenes.clone(),
            chunk_exponent: self.chunk_exponent,
            character_biblio: self.character_biblio.clone(),
            scenes: scenes_data,
        };

        Ok((room_data, all_chunks))
    }
}
