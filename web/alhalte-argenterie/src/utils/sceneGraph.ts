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
 */
export function buildSceneGraph(
  movements: SceneMovementEvent[],
  sceneNames: Map<string, string>,
  accessibleSceneIds: Set<string>,
): SceneGraphResult {
  // Sort events chronologically: earlier first (top), later last (bottom)
  const sorted = [...movements].sort((a, b) => a.timestamp - b.timestamp);

  // Connected components / branch lane calculation
  // Group scenes by connectivity
  const sceneConnections = new Map<string, Set<string>>();
  for (const move of sorted) {
    const from = move.fromSceneId;
    const to = move.toSceneId;
    if (from) {
      if (!sceneConnections.has(from)) sceneConnections.set(from, new Set());
      if (!sceneConnections.has(to)) sceneConnections.set(to, new Set());
      sceneConnections.get(from)!.add(to);
      sceneConnections.get(to)!.add(from);
    } else {
      if (!sceneConnections.has(to)) sceneConnections.set(to, new Set());
    }
  }

  // Find connected components
  const visitedForBranches = new Set<string>();
  const sceneBranchMap = new Map<string, number>();
  let nextBranchIndex = 0;

  for (const sceneId of sceneConnections.keys()) {
    if (visitedForBranches.has(sceneId)) continue;

    const queue = [sceneId];
    visitedForBranches.add(sceneId);

    while (queue.length > 0) {
      const current = queue.shift()!;
      sceneBranchMap.set(current, nextBranchIndex);

      const neighbors = sceneConnections.get(current);
      if (neighbors) {
        for (const neighbor of neighbors) {
          if (!visitedForBranches.has(neighbor)) {
            visitedForBranches.add(neighbor);
            queue.push(neighbor);
          }
        }
      }
    }

    nextBranchIndex++;
  }

  // If there are scenes without connections, assign each to a branch
  for (const move of sorted) {
    if (!sceneBranchMap.has(move.toSceneId)) {
      sceneBranchMap.set(move.toSceneId, nextBranchIndex++);
    }
  }

  const nodes: SceneGraphNode[] = [];
  const edges: SceneGraphEdge[] = [];

  // Track the latest node created for each character: characterId -> nodeId
  const characterLastNodeId = new Map<string, string>();

  // Track scenes visited per character to detect duplicates on revisit
  const characterVisitedScenes = new Map<string, Set<string>>();

  // Cache for unknown scene "?" nodes: unknownSceneId -> nodeId
  const unknownSceneNodes = new Map<string, string>();

  let nodeCounter = 0;

  for (const move of sorted) {
    const { characterId, characterName, fromSceneId, toSceneId, timestamp } = move;

    let characterHistory = characterVisitedScenes.get(characterId);
    if (!characterHistory) {
      characterHistory = new Set();
      characterVisitedScenes.set(characterId, characterHistory);
    }

    const isKnown = accessibleSceneIds.has(toSceneId);

    let targetNodeId: string;

    if (!isKnown) {
      // Inaccessible scene: single "?" node per unknown scene
      if (unknownSceneNodes.has(toSceneId)) {
        targetNodeId = unknownSceneNodes.get(toSceneId)!;
      } else {
        targetNodeId = `node-unknown-${++nodeCounter}`;
        unknownSceneNodes.set(toSceneId, targetNodeId);

        const branch = sceneBranchMap.get(toSceneId) ?? nextBranchIndex++;
        nodes.push({
          id: targetNodeId,
          sceneId: toSceneId,
          sceneName: '？',
          timestamp,
          characterId,
          characterName,
          branchIndex: branch,
          isUnknown: true,
        });
      }

      // Add dotted edge from previous node if any
      const prevNodeId = characterLastNodeId.get(characterId);
      if (prevNodeId) {
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
    const isRevisit = characterHistory.has(toSceneId);
    characterHistory.add(toSceneId);

    targetNodeId = `node-${toSceneId}-${++nodeCounter}`;
    const branch = sceneBranchMap.get(toSceneId) ?? 0;
    const sceneName = sceneNames.get(toSceneId) ?? toSceneId;

    nodes.push({
      id: targetNodeId,
      sceneId: toSceneId,
      sceneName,
      timestamp,
      characterId,
      characterName,
      branchIndex: branch,
      isDuplicate: isRevisit,
      isUnknown: false,
    });

    // Connect from previous node of this character
    const prevNodeId = characterLastNodeId.get(characterId);
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

  return {
    nodes,
    edges,
    branchCount: Math.max(1, nextBranchIndex),
  };
}
