/**
 * Scene graph construction algorithm (VSCode Git Graph-like view).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーングラフタブ`
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### log (character-came, character-gone)`
 */

import type {
  LogRecord,
  SceneGraphEdge,
  SceneGraphNode,
  SceneGraphResult,
} from '../types/index.js';

export interface SceneMovementEvent {
  characterId: string;
  characterName: string;
  fromSceneId?: string; // If undefined, initial spawn in toSceneId
  toSceneId: string;
  timestamp: number;
  /**
   * Source of the movement. `log` events are reconstructed from
   * `character-came` / `character-gone` records; `live` events come from a
   * `CharacterMoved` broadcast. Used by {@link reconcileMovements}.
   */
  source?: 'log' | 'live';
  /** Id of the source log record (stable dedupe key for reconstructed moves). */
  id?: string;
  /** Timeline position among log events (ties on the timestamp-less events). */
  order?: number;
}

/**
 * Keeps only the movements performed by the given characters (Issue #263).
 *
 * The scene graph is scoped to the caller's own characters: a participant
 * should not learn the route of characters they do not own.
 */
export function filterOwnedMovements(
  movements: SceneMovementEvent[],
  ownedCharacterIds: ReadonlySet<string>,
): SceneMovementEvent[] {
  if (ownedCharacterIds.size === 0) return [];
  return movements.filter((movement) => ownedCharacterIds.has(movement.characterId));
}

/**
 * Computes `V_known` for the scene graph (Issue #263): the scenes the caller's
 * own characters have participated in, plus the seed scenes (the room root, the
 * scene currently being viewed and the scenes where an owned character is
 * currently present).
 *
 * Only ids present in `sceneIds` are returned. A movement to a scene that no
 * longer exists therefore becomes a frontier edge to an anonymous "?" node,
 * not a named node. The result is recomputed on every call; it is never cached.
 */
export function collectKnownSceneIds(
  movements: SceneMovementEvent[],
  sceneIds: ReadonlySet<string>,
  seedSceneIds: Iterable<string> = [],
): Set<string> {
  const known = new Set<string>();
  for (const sceneId of seedSceneIds) {
    if (sceneIds.has(sceneId)) known.add(sceneId);
  }
  for (const movement of movements) {
    if (sceneIds.has(movement.toSceneId)) known.add(movement.toSceneId);
    if (movement.fromSceneId !== undefined && sceneIds.has(movement.fromSceneId)) {
      known.add(movement.fromSceneId);
    }
  }
  return known;
}

/**
 * Builds a Git Graph representation from character movement events.
 *
 * Requirements:
 * 1. Earlier events at top, later events at bottom (chronological ascending order).
 * 2. Uncorrelated scenes are assigned to separate branch lanes.
 * 3. A movement from a known scene to a scene outside `accessibleSceneIds` is
 *    drawn as a dotted edge to an independent "?" node. The unknown scene id and
 *    name are not exposed, and two frontier edges never share a node (Issue #263).
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
  // Sort events chronologically: earlier first (top), later last (bottom).
  // System events have no timestamp (0), so the reconstructed log order breaks
  // ties instead of shuffling all of them to the top.
  const sorted = [...movements].sort((a, b) => {
    if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
    return (a.order ?? 0) - (b.order ?? 0);
  });

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

    // Resolve parent node. An unknown node is never a parent/route: once a
    // character walks out of a frontier scene, only the fact that some unknown
    // scene was involved is known, not where the character came from.
    let prevNodeId: string | undefined;
    const charLastId = characterLastNodeId.get(characterId);
    const charLastNode = charLastId ? nodeById.get(charLastId) : undefined;
    const knownCharLastNode = charLastNode && !charLastNode.isUnknown ? charLastNode : undefined;

    if (fromSceneId) {
      if (knownCharLastNode && knownCharLastNode.sceneId === fromSceneId) {
        prevNodeId = knownCharLastNode.id;
      } else if (accessibleSceneIds.has(fromSceneId) && sceneLatestNodeId.has(fromSceneId)) {
        prevNodeId = sceneLatestNodeId.get(fromSceneId);
      } else if (accessibleSceneIds.has(fromSceneId)) {
        // Initial node for a known fromSceneId needs to be created on lane 0
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
      }
    } else {
      prevNodeId = knownCharLastNode?.id;
    }

    const prevNode = prevNodeId ? nodeById.get(prevNodeId) : undefined;
    if (prevNode) {
      nodeHasOutbound.add(prevNode.id);
      charactersAtNode.get(prevNode.id)?.delete(characterId);
    }

    const isKnown = accessibleSceneIds.has(toSceneId);

    if (!isKnown) {
      if (!prevNodeId) {
        // The character came from a scene outside V_known: there is no known
        // source to draw a frontier edge from, so draw nothing rather than an
        // orphan "?" node.
        characterLastNodeId.delete(characterId);
        continue;
      }

      // Frontier edge: create an independent "?" node per transition. Neither
      // the unknown scene id nor its name is retained, and two edges never
      // merge into one node (Issue #263).
      const targetNodeId = `node-unknown-${++nodeCounter}`;
      const branch = allocateLane(prevNode);
      const unknownNode: SceneGraphNode = {
        id: targetNodeId,
        sceneId: '',
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
      charactersAtNode.set(targetNodeId, new Set());

      edges.push({
        fromNodeId: prevNodeId,
        toNodeId: targetNodeId,
        characterId,
        isDotted: true,
      });

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

/**
 * Rebuilds character movements from banquette `character-came` /
 * `character-gone` system log records (Issue #162).
 *
 * The server appends a `gone` record in the source scene and a `came` record
 * in the destination scene, in that order. Pairing them gives the movement
 * `from -> to`. When the records arrive out of order (each scene has its own
 * chunk position), a `gone` retroactively fills in the origin of the latest
 * came, and a `came` without any `gone` is treated as an initial spawn.
 *
 * System events carry an append timestamp since Issue #227, but
 * `(chunkIndex, position)` stays the authoritative per-scene order and is
 * compared first; `order` preserves that log order for the graph / chronological
 * sort.
 */
export function buildMovementsFromLogRecords(
  records: LogRecord[],
  characterNames: Map<string, string> = new Map(),
): SceneMovementEvent[] {
  const seenRecordIds = new Set<string>();
  const events: Array<{ record: LogRecord; index: number }> = [];
  for (const record of records) {
    if (record.kind !== 'system') continue;
    const event = record.event;
    if (!event || (event.type !== 'character-came' && event.type !== 'character-gone')) {
      continue;
    }
    if (seenRecordIds.has(record.id)) continue;
    seenRecordIds.add(record.id);
    events.push({ record, index: events.length });
  }

  events.sort((a, b) => {
    const aIndex = a.record.chunkIndex;
    const bIndex = b.record.chunkIndex;
    if (typeof aIndex === 'number' && typeof bIndex === 'number') {
      if (aIndex !== bIndex) return aIndex - bIndex;
      const aPosition = a.record.position ?? 0;
      const bPosition = b.record.position ?? 0;
      if (aPosition !== bPosition) return aPosition - bPosition;
    } else if (typeof aIndex === 'number') {
      return -1;
    } else if (typeof bIndex === 'number') {
      return 1;
    }
    if (a.record.timestamp !== b.record.timestamp) {
      return a.record.timestamp - b.record.timestamp;
    }
    return a.index - b.index;
  });

  const movements: SceneMovementEvent[] = [];
  const pendingGone = new Map<string, string>();
  const lastSceneByCharacter = new Map<string, string>();
  const lastMovementByCharacter = new Map<string, SceneMovementEvent>();

  for (let order = 0; order < events.length; order++) {
    const record = events[order].record;
    const event = record.event;
    if (!event || (event.type !== 'character-came' && event.type !== 'character-gone')) {
      continue;
    }
    const characterId = event.characterId;
    if (event.type === 'character-gone') {
      const last = lastMovementByCharacter.get(characterId);
      // Out-of-order (came before gone): this gone is the origin of the most
      // recent movement into a different scene.
      if (last && last.fromSceneId === undefined && last.toSceneId !== record.sceneId) {
        last.fromSceneId = record.sceneId;
      } else {
        pendingGone.set(characterId, record.sceneId);
      }
      lastSceneByCharacter.set(characterId, record.sceneId);
      continue;
    }

    const fromSceneId =
      pendingGone.get(characterId) ??
      (lastSceneByCharacter.get(characterId) !== record.sceneId
        ? lastSceneByCharacter.get(characterId)
        : undefined);
    pendingGone.delete(characterId);
    lastSceneByCharacter.set(characterId, record.sceneId);

    const movement: SceneMovementEvent = {
      characterId,
      characterName: characterNames.get(characterId) ?? characterId,
      ...(fromSceneId !== undefined ? { fromSceneId } : {}),
      toSceneId: record.sceneId,
      timestamp: record.timestamp,
      source: 'log',
      id: record.id,
      order,
    };
    movements.push(movement);
    lastMovementByCharacter.set(characterId, movement);
  }

  return movements;
}

/** Key used to dedupe one reconstructed log movement against a live one. */
function exactMovementKey(movement: SceneMovementEvent): string {
  return `${movement.characterId}|${movement.fromSceneId ?? ''}|${movement.toSceneId}`;
}

/** Loose key for came-only log events (origin unknown). */
function looseMovementKey(movement: SceneMovementEvent): string {
  return `${movement.characterId}|${movement.toSceneId}`;
}

function compareMovements(a: SceneMovementEvent, b: SceneMovementEvent): number {
  if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
  return (a.order ?? 0) - (b.order ?? 0);
}

/**
 * Merges live `CharacterMoved` movements with movements reconstructed from the
 * came/gone log records without double-counting a move (Issue #162).
 *
 * A live move is dropped when a reconstructed movement with the same character
 * and destination exists. The match is exact on `from -> to` first; because a
 * came-only record has no known origin, it falls back to `character -> to`.
 * Duplicate live events (the requester gets both the ack and the broadcast) are
 * collapsed by the same multiplicity matching.
 */
export function reconcileMovements(
  liveMovements: SceneMovementEvent[],
  logMovements: SceneMovementEvent[],
): SceneMovementEvent[] {
  const exactCounts = new Map<string, number>();
  const anyCounts = new Map<string, number>();
  const unknownOriginCounts = new Map<string, number>();
  for (const movement of logMovements) {
    const exact = exactMovementKey(movement);
    exactCounts.set(exact, (exactCounts.get(exact) ?? 0) + 1);
    const loose = looseMovementKey(movement);
    anyCounts.set(loose, (anyCounts.get(loose) ?? 0) + 1);
    if (movement.fromSceneId === undefined) {
      unknownOriginCounts.set(loose, (unknownOriginCounts.get(loose) ?? 0) + 1);
    }
  }

  const consume = (movement: SceneMovementEvent) => {
    const loose = looseMovementKey(movement);
    anyCounts.set(loose, (anyCounts.get(loose) ?? 0) - 1);
  };

  const keptLive: SceneMovementEvent[] = [];
  // The requester can receive the same `CharacterMoved` as both an ack and a
  // broadcast; collapse live duplicates before matching them against the log.
  for (const movement of collapseLiveMovements(liveMovements)) {
    const exactKey = exactMovementKey(movement);
    const exact = exactCounts.get(exactKey) ?? 0;
    if (exact > 0) {
      exactCounts.set(exactKey, exact - 1);
      consume(movement);
      const unknownKey = looseMovementKey(movement);
      const unknown = unknownOriginCounts.get(unknownKey) ?? 0;
      if (unknown > 0) unknownOriginCounts.set(unknownKey, unknown - 1);
      continue;
    }

    const loose = looseMovementKey(movement);
    // A membership summary may not have hydrated the source scene yet, so a
    // live move can have no origin while the log record does. Match on
    // character + destination when either side lacks the origin.
    const unknown = unknownOriginCounts.get(loose) ?? 0;
    const any = anyCounts.get(loose) ?? 0;
    if ((movement.fromSceneId === undefined && any > 0) || unknown > 0) {
      consume(movement);
      if (unknown > 0) unknownOriginCounts.set(loose, unknown - 1);
      continue;
    }
    keptLive.push(movement);
  }

  return [...logMovements, ...keptLive].sort(compareMovements);
}

/**
 * Collapses duplicate live movements of the same character / route.
 *
 * The requester of a `MoveCharacter` receives the `CharacterMoved` ack and
 * also appends the movement from the resolved request; both call
 * {@link collapseLiveMovements}. A short window keeps a genuine return to the
 * same route a moment later from being swallowed.
 */
export function collapseLiveMovements(
  movements: SceneMovementEvent[],
  windowSeconds = 5,
): SceneMovementEvent[] {
  const result: SceneMovementEvent[] = [];
  for (const movement of movements) {
    const duplicate = result.some(
      (previous) =>
        previous.characterId === movement.characterId &&
        previous.fromSceneId === movement.fromSceneId &&
        previous.toSceneId === movement.toSceneId &&
        Math.abs(previous.timestamp - movement.timestamp) <= windowSeconds,
    );
    if (!duplicate) result.push(movement);
  }
  return result;
}
