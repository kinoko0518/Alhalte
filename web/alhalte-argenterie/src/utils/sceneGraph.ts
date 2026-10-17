/**
 * Scene graph construction algorithm (VSCode Git Graph-like view).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーングラフタブ`
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### log (character-came, character-gone)`
 */

import type { SceneGraphEdge, SceneGraphNode, SceneGraphResult } from '../types/index.js';

export interface SceneMovementEvent {
  characterId: string;
  characterName: string;
  fromSceneId?: string; // If undefined, initial spawn in toSceneId
  toSceneId: string;
  timestamp: number;
}

/**
 * Builds a Git Graph representation from character movement events.
 *
 * Requirements:
 * 1. Earlier events at top, later events at bottom (chronological ascending order).
 * 2. Uncorrelated scenes are assigned to separate branch lanes.
 * 3. Movements to inaccessible/unknown scenes are drawn to a "?" node via a dotted edge (one "?" node per unknown scene).
 * 4. Re-visiting the same scene duplicates the node.
 * 5. Branch continuations remain strictly vertical (parallel to Y axis, branchIndex === parent.branchIndex).
 * 6. Branch merges converge back into the minimum branch lane.
 * 7. Initial root node for '/' ("メインシーン") is placed at the top on lane 0.
 */
export function buildSceneGraph(
  movements: SceneMovementEvent[],
  sceneNames: Map<string, string>,
  accessibleSceneIds: Set<string>,
): SceneGraphResult {
  // Sort events chronologically: earlier first (top), later last (bottom)
  const sorted = [...movements].sort((a, b) => a.timestamp - b.timestamp);

  const nodes: SceneGraphNode[] = [];
  const edges: SceneGraphEdge[] = [];
  const nodeById = new Map<string, SceneGraphNode>();

  // Track active latest node for each scene: sceneId -> nodeId
  const sceneLatestNodeId = new Map<string, string>();

  // Track the latest node created/visited for each character: characterId -> nodeId
  const characterLastNodeId = new Map<string, string>();

  // Track scenes visited per character to detect duplicates on revisit: characterId -> Set<sceneId>
  const characterVisitedScenes = new Map<string, Set<string>>();

  // Track specific nodes visited per character: characterId -> Set<nodeId>
  const characterVisitedNodes = new Map<string, Set<string>>();

  // Track which nodes have outbound movements: Set<nodeId>
  const nodeHasOutbound = new Set<string>();

  // Track characters currently present at each node: nodeId -> Set<characterId>
  const charactersAtNode = new Map<string, Set<string>>();

  // Track children count for each node: nodeId -> number
  const nodeChildCount = new Map<string, number>();

  // Track currently active branch lanes: Set<number>
  const activeLanes = new Set<number>();

  // Cache for unknown scene "?" nodes: unknownSceneId -> nodeId
  const unknownSceneNodes = new Map<string, string>();

  let nodeCounter = 0;

  function allocateLane(parentNode?: SceneGraphNode): number {
    if (!parentNode) {
      // Find smallest unused lane
      let lane = 0;
      while (activeLanes.has(lane)) {
        lane++;
      }
      activeLanes.add(lane);
      return lane;
    }

    const currentChildren = nodeChildCount.get(parentNode.id) ?? 0;
    if (currentChildren === 0) {
      // First child inherits parent's lane (vertical continuation parallel to Y axis)
      nodeChildCount.set(parentNode.id, 1);
      activeLanes.add(parentNode.branchIndex);
      return parentNode.branchIndex;
    }

    // Branching: allocate new lane > parentNode.branchIndex
    let lane = parentNode.branchIndex + 1;
    while (activeLanes.has(lane)) {
      lane++;
    }
    nodeChildCount.set(parentNode.id, currentChildren + 1);
    activeLanes.add(lane);
    return lane;
  }

  // 1. Initial Root Node for '/' (メインシーン)
  // Include '/' when accessibleSceneIds has '/' or sceneNames has '/' or movements mention '/'
  // or when movements is empty and both sets are empty (default state)
  const shouldIncludeRoot =
    accessibleSceneIds.has('/') ||
    sceneNames.has('/') ||
    movements.some((m) => m.fromSceneId === '/' || m.toSceneId === '/') ||
    (movements.length === 0 && accessibleSceneIds.size === 0 && sceneNames.size === 0);

  if (shouldIncludeRoot) {
    const rootTimestamp =
      sorted.length > 0
        ? sorted[0].timestamp > 0
          ? sorted[0].timestamp - 1
          : 0
        : Math.floor(Date.now() / 1000);
    const rootNode: SceneGraphNode = {
      id: 'node-/-root',
      sceneId: '/',
      sceneName: sceneNames.get('/') ?? 'メインシーン',
      timestamp: rootTimestamp,
      characterId: '',
      characterName: '',
      branchIndex: 0,
      isUnknown: false,
      isDuplicate: false,
    };
    nodes.push(rootNode);
    nodeById.set(rootNode.id, rootNode);
    sceneLatestNodeId.set('/', rootNode.id);
    nodeChildCount.set(rootNode.id, 0);
    activeLanes.add(0);
    charactersAtNode.set(rootNode.id, new Set());
  }

  for (const move of sorted) {
    const { characterId, characterName, fromSceneId, toSceneId, timestamp } = move;

    let charScenes = characterVisitedScenes.get(characterId);
    if (!charScenes) {
      charScenes = new Set();
      characterVisitedScenes.set(characterId, charScenes);
    }
    let charNodes = characterVisitedNodes.get(characterId);
    if (!charNodes) {
      charNodes = new Set();
      characterVisitedNodes.set(characterId, charNodes);
    }

    // Resolve parent node
    let prevNodeId: string | undefined;
    const charLastId = characterLastNodeId.get(characterId);
    const charLastNode = charLastId ? nodeById.get(charLastId) : undefined;

    if (fromSceneId) {
      if (charLastNode && charLastNode.sceneId === fromSceneId) {
        prevNodeId = charLastNode.id;
      } else if (sceneLatestNodeId.has(fromSceneId)) {
        prevNodeId = sceneLatestNodeId.get(fromSceneId);
      } else if (accessibleSceneIds.has(fromSceneId) || sceneNames.has(fromSceneId)) {
        // Initial node for fromSceneId needs to be created on lane 0
        const fromNodeId = `node-${fromSceneId}-init`;
        const fromNode: SceneGraphNode = {
          id: fromNodeId,
          sceneId: fromSceneId,
          sceneName: sceneNames.get(fromSceneId) ?? fromSceneId,
          timestamp: timestamp > 0 ? timestamp - 1 : 0,
          characterId,
          characterName,
          branchIndex: allocateLane(),
          isUnknown: false,
        };
        nodes.push(fromNode);
        nodeById.set(fromNodeId, fromNode);
        sceneLatestNodeId.set(fromSceneId, fromNodeId);
        nodeChildCount.set(fromNodeId, 0);
        charactersAtNode.set(fromNodeId, new Set([characterId]));
        charScenes.add(fromSceneId);
        charNodes.add(fromNodeId);
        prevNodeId = fromNodeId;
      } else {
        prevNodeId = charLastId ?? sceneLatestNodeId.get('/');
      }
    } else {
      prevNodeId = charLastId;
    }

    const prevNode = prevNodeId ? nodeById.get(prevNodeId) : undefined;
    if (prevNode) {
      nodeHasOutbound.add(prevNode.id);
      charactersAtNode.get(prevNode.id)?.delete(characterId);
    }

    const isKnown = accessibleSceneIds.has(toSceneId);

    if (!isKnown) {
      // Inaccessible scene: single "?" node per unknown scene
      let targetNodeId: string;
      if (unknownSceneNodes.has(toSceneId)) {
        targetNodeId = unknownSceneNodes.get(toSceneId)!;
        const targetNode = nodeById.get(targetNodeId)!;
        targetNode.timestamp = Math.max(targetNode.timestamp, timestamp);
      } else {
        targetNodeId = `node-unknown-${++nodeCounter}`;
        unknownSceneNodes.set(toSceneId, targetNodeId);

        const branch = allocateLane(prevNode);
        const unknownNode: SceneGraphNode = {
          id: targetNodeId,
          sceneId: toSceneId,
          sceneName: '？',
          timestamp,
          characterId,
          characterName,
          branchIndex: branch,
          isUnknown: true,
        };
        nodes.push(unknownNode);
        nodeById.set(targetNodeId, unknownNode);
        nodeChildCount.set(targetNodeId, 0);
      }

      if (prevNodeId) {
        if (prevNode) {
          nodeChildCount.set(prevNode.id, (nodeChildCount.get(prevNode.id) ?? 0) + 1);
        }
        edges.push({
          fromNodeId: prevNodeId,
          toNodeId: targetNodeId,
          characterId,
          isDotted: true,
        });
      }

      characterLastNodeId.set(characterId, targetNodeId);
      continue;
    }

    // Accessible scene
    const existingNodeId = sceneLatestNodeId.get(toSceneId);
    const existingNode = existingNodeId ? nodeById.get(existingNodeId) : undefined;

    // Check if this character is returning to a previously visited scene
    const isRevisit = charScenes.has(toSceneId);

    // Can we merge into existingNode?
    const canMerge =
      existingNode &&
      !existingNode.isUnknown &&
      !isRevisit &&
      !nodeHasOutbound.has(existingNode.id) &&
      prevNodeId !== undefined &&
      prevNodeId !== existingNode.id &&
      !charactersAtNode.get(existingNode.id)?.has(characterId) &&
      !charNodes.has(existingNode.id);

    if (canMerge && existingNode && prevNode) {
      // Merge into existingNode
      edges.push({
        fromNodeId: prevNode.id,
        toNodeId: existingNode.id,
        characterId,
        isDotted: false,
      });

      nodeChildCount.set(prevNode.id, (nodeChildCount.get(prevNode.id) ?? 0) + 1);

      // Node placed on minimum branchIndex among its parents
      const higherBranch = Math.max(existingNode.branchIndex, prevNode.branchIndex);
      const minBranch = Math.min(existingNode.branchIndex, prevNode.branchIndex);
      existingNode.branchIndex = minBranch;

      // Free the higher lane if it was active
      if (higherBranch !== minBranch) {
        activeLanes.delete(higherBranch);
      }

      // Update timestamp and move existingNode to the end of nodes (bottom row)
      existingNode.timestamp = Math.max(existingNode.timestamp, timestamp);
      const existingIdx = nodes.indexOf(existingNode);
      if (existingIdx !== -1) {
        nodes.splice(existingIdx, 1);
        nodes.push(existingNode);
      }

      charScenes.add(toSceneId);
      charNodes.add(existingNode.id);
      charactersAtNode.get(existingNode.id)?.add(characterId);
      characterLastNodeId.set(characterId, existingNode.id);
      continue;
    }

    // Create new node for toSceneId
    charScenes.add(toSceneId);

    const targetNodeId = `node-${toSceneId}-${++nodeCounter}`;
    const branch = allocateLane(prevNode);
    const sceneName = sceneNames.get(toSceneId) ?? toSceneId;

    const newNode: SceneGraphNode = {
      id: targetNodeId,
      sceneId: toSceneId,
      sceneName,
      timestamp,
      characterId,
      characterName,
      branchIndex: branch,
      isDuplicate: isRevisit,
      isUnknown: false,
    };

    nodes.push(newNode);
    nodeById.set(targetNodeId, newNode);
    nodeChildCount.set(targetNodeId, 0);
    sceneLatestNodeId.set(toSceneId, targetNodeId);
    charactersAtNode.set(targetNodeId, new Set([characterId]));
    charNodes.add(targetNodeId);

    if (prevNodeId) {
      edges.push({
        fromNodeId: prevNodeId,
        toNodeId: targetNodeId,
        characterId,
        isDotted: false,
      });
    }

    characterLastNodeId.set(characterId, targetNodeId);
  }

  const maxBranch = nodes.reduce((max, n) => Math.max(max, n.branchIndex), 0);

  return {
    nodes,
    edges,
    branchCount: nodes.length > 0 ? maxBranch + 1 : 1,
  };
}
