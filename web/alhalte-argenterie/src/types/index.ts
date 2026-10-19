/**
 * Types and interfaces for `@alhalte/argenterie`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie`, `# 拡張機能`
 */

/**
 * Visited room record stored in localStorage.
 */
export interface VisitedRoom {
  id: string;
  name: string;
  url: string;
  lastVisited: number;
  isAdmin?: boolean;
}

/**
 * 2D point / transform representation.
 */
export interface ViewTransform {
  x: number;
  y: number;
  scale: number;
}

/**
 * Status attribute for a character (e.g. HP, MP, SAN).
 */
export interface CharacterStatus {
  label: string;
  current: number;
  max: number;
}

/**
 * Parameter attribute for a character (e.g. STR, DEX, POW).
 */
export interface CharacterParameter {
  label: string;
  value: number;
}

/**
 * Character entity.
 */
export interface Character {
  id: string;
  name: string;
  initiative: number;
  iconUrl?: string;
  color?: string;
  description: string;
  reference?: string;
  statuses: CharacterStatus[];
  parameters: CharacterParameter[];
  chatPalette: string;
  hideStatus: boolean;
  isNarrative?: boolean;
  placedAsPiece?: boolean;
  piecePosition?: { x: number; y: number };
}

/**
 * BGM Slot item for multi-track audio control (Issue #53).
 */
export interface BgmSlot {
  slot: number; // u32, 1-based index (e.g. 1, 2, 3...)
  tinId?: string | null; // Option<string>: null or undefined means None (stopped)
  volume: number; // Volume from 0.0 to 1.0 (represented as 0 to 100 on Slider)
  isPlaying: boolean; // Playback toggle state
  name?: string; // Optional track name / description
}

/**
 * Juices (sound and visual atmosphere settings) for a scene.
 */
export interface Juices {
  bgImage?: string;
  bgWidth?: number;
  bgHeight?: number;
  bgAspectRatio?: number;
  bgm1?: string;
  bgm1Volume: number;
  bgm2?: string;
  bgm2Volume: number;
  bgm3?: string;
  bgm3Volume: number;
  bgmSlots?: BgmSlot[];
}

/**
 * Scene entity.
 */
export interface Scene {
  id: string;
  name: string;
  juices: Juices;
  viewTransform: ViewTransform;
  members: string[]; // Character IDs present in this scene
}

/**
 * Kind of a log record.
 *
 * - `chat`: A user-authored message rendered by Saignant.
 * - `system`: A banquette event (`change-bgm`, `change-bg-img`,
 *   `character-came`, `character-gone`) rendered as a status row.
 */
export type LogRecordKind = 'chat' | 'system';

/** A BGM change system event (`alhalte/banquette/log/change-bgm`). */
export interface LogChangeBgmEvent {
  type: 'change-bgm';
  /** Tin ID of the new track, or `undefined` when playback stopped. */
  bgmTinId?: string;
  /** Playback volume in the `0.0..=1.0` range. */
  volume?: number;
  /** 1-based BGM slot. */
  slot: number;
  /** Legacy track index, when present. */
  trackIndex?: number;
}

/** A background image change system event (`alhalte/banquette/log/change-bg-img`). */
export interface LogChangeBgImgEvent {
  type: 'change-bg-img';
  /** Tin ID of the new image, or `undefined` when the background was cleared. */
  imageTinId?: string;
  /** Optional target width. */
  width?: number;
  /** Optional target height. */
  height?: number;
}

/** A character entrance system event (`alhalte/banquette/log/character-came`). */
export interface LogCharacterCameEvent {
  type: 'character-came';
  characterId: string;
}

/** A character departure system event (`alhalte/banquette/log/character-gone`). */
export interface LogCharacterGoneEvent {
  type: 'character-gone';
  characterId: string;
}

/** Decoded banquette system event carried by a {@link LogRecord}. */
export type LogSystemEvent =
  | LogChangeBgmEvent
  | LogChangeBgImgEvent
  | LogCharacterCameEvent
  | LogCharacterGoneEvent;

/**
 * Log entry parsed from pantry / cellar / banquette.
 *
 * The canonical wire form is the `alhalte/argenterie/log-record` JSON Tin. Old
 * records omit {@link LogRecord.contentTin} and are re-processed through the
 * extension pipeline at render time; records carrying it are rendered as-is so
 * extension version / priority differences cannot change their display.
 */
export interface LogRecord {
  id: string;
  sceneId: string;
  content: string; // Generast string representation (fallback: "")
  speaker: string; // Character name or mention (fallback: "Anonymous")
  speakerId?: string;
  timestamp: number; // Unix timestamp in seconds (fallback: 0)
  chunkId?: string;
  /** Log chunk index this record belongs to (when known). */
  chunkIndex?: number;
  /** Zero-based position inside {@link LogRecord.chunkIndex} (when known). */
  position?: number;
  /**
   * True when the record was restored from the offline `localStorage` cache and
   * has not been replaced by an authoritative server snapshot yet.
   */
  cached?: boolean;
  /**
   * Extension-pipeline output: the `alhalte/generast` Tin plaintext bytes
   * (a `(Generast, speaker_id)` ext-io Tin when produced by the send path).
   * Stored as a byte array in JSON; base64 is also accepted when decoding.
   */
  contentTin?: number[];
  /** Record kind. Missing values on old records are treated as `chat`. */
  kind?: LogRecordKind;
  /** Decoded system event payload; set only when `kind === 'system'`. */
  event?: LogSystemEvent;
}

/**
 * Chunk containing multiple log records.
 */
export interface LogChunk {
  chunkId: string;
  sceneId: string;
  entries: LogRecord[];
  isLoaded: boolean;
  /** Banquette chunk index; absent for live / cache chunks. */
  chunkIndex?: number;
}

/**
 * Extension entity for cutlery plugins.
 */
export interface ExtensionItem {
  id: string;
  name: string;
  iconSvg?: string;
  description?: string;
  enabled: boolean;
  priority: number; // Order index
  wasmBytes?: Uint8Array;
  cellarTinId?: string;
}

/**
 * Scene graph node representing a character's presence or movement in a scene.
 */
export interface SceneGraphNode {
  id: string;
  sceneId: string;
  sceneName: string;
  timestamp: number;
  characterId: string;
  characterName: string;
  branchIndex: number; // Column / lane in Git Graph view
  isUnknown?: boolean; // True for unreachable / inaccessible scene "?" node
  isDuplicate?: boolean; // True if character returned to this scene
}

/**
 * Scene graph directed transition edge.
 */
export interface SceneGraphEdge {
  fromNodeId: string;
  toNodeId: string;
  characterId: string;
  isDotted?: boolean; // Dotted line for transitions to unknown scenes
}

/**
 * Result of building the scene graph.
 */
export interface SceneGraphResult {
  nodes: SceneGraphNode[];
  edges: SceneGraphEdge[];
  branchCount: number;
}

/**
 * Crop and dimension calculation result for Juices background image.
 */
export interface CropCalculationResult {
  width: number;
  height: number;
  aspectRatio: number;
  cropped: boolean;
  cropX: number;
  cropY: number;
  cropWidth: number;
  cropHeight: number;
}

export type {
  GenerastAST,
  GenerastStringNode,
  GenerastElementNode,
  GenerastNestedNode,
} from '../utils/generast.js';

export type {
  ConnectionState,
  ConnectionManagerOptions,
} from '../utils/connection.js';

export type { RecentRoom } from '../utils/recentRooms.js';

export type {
  ClientFrame,
  ServerFrame,
  RoomSocketOptions,
} from '../utils/roomSocket.js';

export type {
  RoomRole,
  AccountSearchItem,
  RoomPermissionItem,
} from '../utils/session.js';


