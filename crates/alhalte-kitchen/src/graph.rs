//! Internal graph representation for rooms, scene stay segments, and character movements.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::ops::Deref;

use alhalte_banquette::Character;
use alhalte_saignant::LogEntry;
use serde::{Deserialize, Serialize};

use crate::archive::{ArchiveScene, ParsedSceneEvent, RoomArchive, SceneEvent};
use crate::error::{KitchenError, Result};
use crate::serialize::{SerializedScene, SerializedSession};

/// Identifier for a node in the room graph.
///
/// A node typically represents a scene or a specific visit segment of a scene.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    /// Creates a new `NodeId` from a string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Creates a composite node ID for a scene and visit index.
    pub fn scene_visit(scene_id: &str, visit_index: usize) -> Self {
        Self(format!("{scene_id}:{visit_index}"))
    }

    /// Returns the string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for NodeId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for NodeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<u32> for NodeId {
    fn from(n: u32) -> Self {
        Self(n.to_string())
    }
}

impl From<usize> for NodeId {
    fn from(n: usize) -> Self {
        Self(n.to_string())
    }
}

/// A node in the [`RoomGraph`] representing a scene or scene stay segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    /// Unique identifier for this graph node.
    pub id: NodeId,
    /// Identifier of the scene associated with this node.
    pub scene_id: String,
    /// Sequential visit index for this scene (0 for initial visit, incremented on return).
    pub visit_index: usize,
    /// Earliest timestamp of an event in this node.
    pub start_time: u64,
    /// Latest timestamp of an event in this node.
    pub end_time: u64,
    /// Characters present during this node/segment.
    pub present_characters: HashSet<u64>,
    /// Chronological log entries belonging to this node.
    pub logs: Vec<LogEntry>,
}

/// A directed edge in the [`RoomGraph`] representing character movement between scenes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Origin node identifier.
    pub from: NodeId,
    /// Destination node identifier.
    pub to: NodeId,
    /// 64-bit character identifier who traversed this edge.
    pub character_id: u64,
    /// Timestamp when transition occurred.
    pub timestamp: u64,
}

/// Directed graph representation of a room session's scenes and character movements.
#[derive(Debug, Clone)]
pub struct RoomGraph {
    /// Room identifier if available.
    pub room_id: Option<u64>,
    /// All nodes in topological/chronological order.
    pub nodes: Vec<GraphNode>,
    /// All directed character transition edges.
    pub edges: Vec<GraphEdge>,
    /// Mapping from character ID to display name.
    pub character_names: HashMap<u64, String>,
    /// Raw character attribute dictionary.
    pub character_biblio: HashMap<u64, Character>,
    /// Original scene archive records.
    pub scenes_original: HashMap<String, ArchiveScene>,
}

impl RoomGraph {
    /// Builds an internal [`RoomGraph`] from a [`RoomArchive`].
    pub fn build(archive: &RoomArchive) -> Result<Self> {
        let mut character_names = HashMap::new();
        for (&cid, ch) in &archive.characters {
            let name = alhalte_banquette::get_character_name(ch)
                .unwrap_or_else(|| format!("Character {cid}"));
            character_names.insert(cid, name);
        }

        // 1. Collect all scene events with timestamps and ordering
        let mut all_events: Vec<ParsedSceneEvent> = Vec::new();
        for scene in archive.scenes.values() {
            let scene_events = scene.parse_all_events();
            all_events.extend(scene_events);
        }

        // Stable sort chronologically
        all_events.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.order.cmp(&b.order))
        });

        // 2. Initialize scenes with initial visit nodes
        let mut nodes: Vec<GraphNode> = Vec::new();
        let mut node_index_map: HashMap<NodeId, usize> = HashMap::new();
        let mut active_node_for_scene: HashMap<String, NodeId> = HashMap::new();
        let mut visit_counts: HashMap<String, usize> = HashMap::new();
        let mut char_location: HashMap<u64, NodeId> = HashMap::new();

        // Sort scene IDs deterministically so node creation order is stable
        let mut scene_ids: Vec<String> = archive.scenes.keys().cloned().collect();
        scene_ids.sort();

        for sid in &scene_ids {
            let scene = &archive.scenes[sid];
            let initial_visit = 0;
            visit_counts.insert(sid.clone(), 0);

            // If there are multiple visits later, we use "sid:0", but we can also alias "sid"
            let node_id = NodeId::scene_visit(sid, initial_visit);
            let mut present = HashSet::new();
            for &cid in &scene.present_characters {
                present.insert(cid);
                char_location.insert(cid, node_id.clone());
            }

            let node = GraphNode {
                id: node_id.clone(),
                scene_id: sid.clone(),
                visit_index: initial_visit,
                start_time: 0,
                end_time: 0,
                present_characters: present,
                logs: Vec::new(),
            };

            let idx = nodes.len();
            node_index_map.insert(node_id.clone(), idx);
            active_node_for_scene.insert(sid.clone(), node_id);
            nodes.push(node);
        }

        let mut edges: Vec<GraphEdge> = Vec::new();

        // 3. Process events chronologically
        for ev in all_events {
            let sid = &ev.scene_id;
            let current_node_id = active_node_for_scene
                .get(sid)
                .cloned()
                .unwrap_or_else(|| NodeId::from(sid.as_str()));

            match ev.event {
                SceneEvent::CharacterCame { character_id } => {
                    // Check if character was in another scene before
                    if let Some(prev_node_id) = char_location.get(&character_id).cloned() {
                        let prev_scene_id = nodes
                            .get(node_index_map.get(&prev_node_id).copied().unwrap_or(0))
                            .map(|n| n.scene_id.clone());

                        if prev_scene_id.as_deref() != Some(sid) {
                            // Character transitioned from prev_node to this scene
                            // Check if this scene was previously left and character is returning
                            let target_node_id = if let Some(last_left) =
                                nodes.iter().find(|n| n.id == prev_node_id)
                                && last_left.scene_id != *sid
                            {
                                // If the current active node already had logs, or if returning to an existing scene
                                current_node_id.clone()
                            } else {
                                current_node_id.clone()
                            };

                            edges.push(GraphEdge {
                                from: prev_node_id,
                                to: target_node_id.clone(),
                                character_id,
                                timestamp: ev.timestamp,
                            });
                        }
                    }

                    // Update character location
                    char_location.insert(character_id, current_node_id.clone());
                    if let Some(&idx) = node_index_map.get(&current_node_id) {
                        nodes[idx].present_characters.insert(character_id);
                        if nodes[idx].start_time == 0 || ev.timestamp < nodes[idx].start_time {
                            nodes[idx].start_time = ev.timestamp;
                        }
                        if ev.timestamp > nodes[idx].end_time {
                            nodes[idx].end_time = ev.timestamp;
                        }
                    }
                }

                SceneEvent::CharacterGone { character_id } => {
                    if let Some(&idx) = node_index_map.get(&current_node_id) {
                        nodes[idx].present_characters.remove(&character_id);
                        if ev.timestamp > nodes[idx].end_time {
                            nodes[idx].end_time = ev.timestamp;
                        }
                    }
                }

                SceneEvent::Speech(log_entry) => {
                    // If speaker is recorded, verify location transition
                    if let Some(speaker_id) = log_entry.speaker_id() {
                        if let Some(prev_node_id) = char_location.get(&speaker_id).cloned() {
                            let prev_scene_id = nodes
                                .get(node_index_map.get(&prev_node_id).copied().unwrap_or(0))
                                .map(|n| n.scene_id.clone());

                            if prev_scene_id.as_deref() != Some(sid) {
                                // Implicit transition via speaking in new scene
                                edges.push(GraphEdge {
                                    from: prev_node_id,
                                    to: current_node_id.clone(),
                                    character_id: speaker_id,
                                    timestamp: ev.timestamp,
                                });
                            }
                        }

                        char_location.insert(speaker_id, current_node_id.clone());
                        if let Some(&idx) = node_index_map.get(&current_node_id) {
                            nodes[idx].present_characters.insert(speaker_id);
                        }
                    }

                    if let Some(&idx) = node_index_map.get(&current_node_id) {
                        if nodes[idx].start_time == 0 || ev.timestamp < nodes[idx].start_time {
                            nodes[idx].start_time = ev.timestamp;
                        }
                        if ev.timestamp > nodes[idx].end_time {
                            nodes[idx].end_time = ev.timestamp;
                        }
                        nodes[idx].logs.push(log_entry);
                    }
                }

                SceneEvent::ChangeBgImg(_) | SceneEvent::ChangeBgm(_) => {
                    // Update timestamp on current node
                    if let Some(&idx) = node_index_map.get(&current_node_id) {
                        if nodes[idx].start_time == 0 || ev.timestamp < nodes[idx].start_time {
                            nodes[idx].start_time = ev.timestamp;
                        }
                        if ev.timestamp > nodes[idx].end_time {
                            nodes[idx].end_time = ev.timestamp;
                        }
                    }
                }
            }
        }

        Ok(Self {
            room_id: archive.room_id,
            nodes,
            edges,
            character_names,
            character_biblio: archive.characters.clone(),
            scenes_original: archive.scenes.clone(),
        })
    }

    /// Retrieves a graph node by its [`NodeId`].
    pub fn node(&self, id: &NodeId) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == *id)
    }

    /// Resolves a node query (by node ID or scene ID) to a canonical [`NodeId`].
    pub fn resolve_node_id(&self, query: &NodeId) -> Option<NodeId> {
        // Direct node id match
        if self.nodes.iter().any(|n| n.id == *query) {
            return Some(query.clone());
        }

        // Scene ID match (find first node matching scene ID)
        if let Some(n) = self.nodes.iter().find(|n| n.scene_id == query.as_str()) {
            return Some(n.id.clone());
        }

        None
    }

    /// Finds all nodes associated with a specific scene identifier.
    pub fn nodes_for_scene(&self, scene_id: &str) -> Vec<&GraphNode> {
        self.nodes
            .iter()
            .filter(|n| n.scene_id == scene_id)
            .collect()
    }

    /// Finds the first node associated with a scene identifier.
    pub fn find_node_by_scene(&self, scene_id: &str) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.scene_id == scene_id)
    }

    /// Resolves human-readable name for a character ID.
    pub fn character_name(&self, character_id: u64) -> Option<String> {
        self.character_names.get(&character_id).cloned()
    }

    /// Finds a directed path from `start` node to `end` node traversed by `character_id`.
    ///
    /// Searches using breadth-first search along edges attributed to `character_id`.
    pub fn find_path_for_character(
        &self,
        character_id: u64,
        start: &NodeId,
        end: &NodeId,
    ) -> Option<Vec<NodeId>> {
        let start_id = self.resolve_node_id(start)?;
        let end_id = self.resolve_node_id(end)?;

        if start_id == end_id {
            return Some(vec![start_id]);
        }

        let mut queue = VecDeque::new();
        queue.push_back(vec![start_id.clone()]);
        let mut visited = HashSet::new();
        visited.insert(start_id.clone());

        while let Some(path) = queue.pop_front() {
            let curr = path.last().unwrap();
            if curr == &end_id || self.node(curr).map(|n| n.scene_id.as_str()) == Some(end.as_str())
            {
                return Some(path);
            }

            for edge in &self.edges {
                if edge.from == *curr
                    && edge.character_id == character_id
                    && !visited.contains(&edge.to)
                {
                    visited.insert(edge.to.clone());
                    let mut next_path = path.clone();
                    next_path.push(edge.to.clone());
                    queue.push_back(next_path);
                }
            }
        }

        None
    }

    /// Serializes logs along a character's path from `start_node` to `end_node`.
    pub fn serialize_by_character_path(
        &self,
        character_id: u64,
        start_node: impl Into<NodeId>,
        end_node: impl Into<NodeId>,
    ) -> Result<SerializedSession> {
        let start = start_node.into();
        let end = end_node.into();

        let path = self
            .find_path_for_character(character_id, &start, &end)
            .ok_or_else(|| KitchenError::PathNotFound {
                character_id,
                start: start.clone(),
                end: end.clone(),
            })?;

        let char_name = self
            .character_name(character_id)
            .unwrap_or_else(|| format!("Character {character_id}"));
        let title = format!("Journey of {char_name}: {start} to {end}");
        let mut session = SerializedSession::new(title);
        session.character_names = self.character_names.clone();

        for node_id in path {
            let node = self
                .node(&node_id)
                .ok_or_else(|| KitchenError::NodeNotFound(node_id.clone()))?;

            let scene_title = if node.visit_index == 0 {
                node.scene_id.clone()
            } else {
                format!("{} (Visit {})", node.scene_id, node.visit_index + 1)
            };

            session.add_scene(SerializedScene {
                scene_id: node.scene_id.clone(),
                title: scene_title,
                logs: node.logs.clone(),
            });
        }

        Ok(session)
    }

    /// Serializes logs by an explicit list of scene IDs in order.
    pub fn serialize_by_scene_ids<S: AsRef<str>>(
        &self,
        scene_ids: &[S],
    ) -> Result<SerializedSession> {
        let title = if scene_ids.is_empty() {
            "Empty Session".to_string()
        } else {
            format!(
                "Scenes: {}",
                scene_ids
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let mut session = SerializedSession::new(title);
        session.character_names = self.character_names.clone();

        for sid in scene_ids {
            let s_str = sid.as_ref();
            let nodes = self.nodes_for_scene(s_str);

            if nodes.is_empty() && !self.scenes_original.contains_key(s_str) {
                return Err(KitchenError::SceneNotFound(s_str.to_string()));
            }

            let mut all_logs = Vec::new();
            if !nodes.is_empty() {
                for n in nodes {
                    all_logs.extend(n.logs.clone());
                }
            } else if let Some(orig) = self.scenes_original.get(s_str) {
                let events = orig.parse_all_events();
                for ev in events {
                    if let SceneEvent::Speech(log) = ev.event {
                        all_logs.push(log);
                    }
                }
            }

            session.add_scene(SerializedScene {
                scene_id: s_str.to_string(),
                title: s_str.to_string(),
                logs: all_logs,
            });
        }

        Ok(session)
    }
}
