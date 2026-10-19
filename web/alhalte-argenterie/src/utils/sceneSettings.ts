/**
 * Scene settings and Juices logic.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成 / メンバーを移動`
 */

import { labelHash } from '@alhalte/pantry';
import type { BgmSlot, Juices, Scene } from '../types/index.js';
import { downloadTin, encodeRoomTin, uploadTin } from './cellarTransfer.js';

/**
 * Creates a default Juices object.
 */
export function createDefaultJuices(): Juices {
  return {
    bgImage: undefined,
    bgWidth: undefined,
    bgHeight: undefined,
    bgAspectRatio: undefined,
    bgm1: undefined,
    bgm1Volume: 0.8,
    bgm2: undefined,
    bgm2Volume: 0.8,
    bgm3: undefined,
    bgm3Volume: 0.8,
    bgmSlots: [
      { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
      { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
      { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
    ],
  };
}

/**
 * Returns normalized BgmSlot array from Juices, generating slots from legacy bgm1..3 if needed.
 */
export function getNormalizedBgmSlots(juices: Juices): BgmSlot[] {
  if (juices.bgmSlots && juices.bgmSlots.length > 0) {
    return juices.bgmSlots;
  }
  return [
    {
      slot: 1,
      tinId: juices.bgm1 ?? null,
      mimeType: juices.bgm1 ? BGM_AUDIO_MIME_FALLBACK : undefined,
      volume: juices.bgm1Volume ?? 0.8,
      isPlaying: Boolean(juices.bgm1),
    },
    {
      slot: 2,
      tinId: juices.bgm2 ?? null,
      mimeType: juices.bgm2 ? BGM_AUDIO_MIME_FALLBACK : undefined,
      volume: juices.bgm2Volume ?? 0.8,
      isPlaying: Boolean(juices.bgm2),
    },
    {
      slot: 3,
      tinId: juices.bgm3 ?? null,
      mimeType: juices.bgm3 ? BGM_AUDIO_MIME_FALLBACK : undefined,
      volume: juices.bgm3Volume ?? 0.8,
      isPlaying: Boolean(juices.bgm3),
    },
  ];
}

/**
 * Inherits Juices from a parent / source scene.
 *
 * ARCHITECTURE.md: "トグル”Juicesを継承”" / "存在しないシーン名が割り振られていた場合、Juicesを継承して新規作成する。"
 */
export function inheritJuices(source: Juices): Juices {
  const result: Juices = {
    bgImage: source.bgImage,
    bgWidth: source.bgWidth,
    bgHeight: source.bgHeight,
    bgAspectRatio: source.bgAspectRatio,
    bgm1: source.bgm1,
    bgm1Volume: source.bgm1Volume,
    bgm2: source.bgm2,
    bgm2Volume: source.bgm2Volume,
    bgm3: source.bgm3,
    bgm3Volume: source.bgm3Volume,
  };
  if (source.bgmSlots) {
    result.bgmSlots = source.bgmSlots.map((s) => ({ ...s }));
  }
  return result;
}

export {
  calculateBackgroundDimensions,
  calculateDimensionsAndCrop,
  type BackgroundDimensionParams,
  type BackgroundDimensionResult,
} from './backgroundDimension.js';


/**
 * Moves a member to target scene. If the target scene name does not exist,
 * automatically creates the scene inheriting Juices from the current scene.
 */
export function moveMemberToScene(
  characterId: string,
  targetSceneName: string,
  currentScene: Scene,
  existingScenes: Scene[],
): {
  updatedScenes: Scene[];
  targetScene: Scene;
  isNewSceneCreated: boolean;
} {
  const trimmedName = targetSceneName.trim();
  const scenes = existingScenes.map((s) => ({
    ...s,
    members: s.members.filter((id) => id !== characterId),
  }));

  let targetScene = scenes.find((s) => s.name === trimmedName);
  let isNewSceneCreated = false;

  if (!targetScene) {
    // Scene does not exist: create new scene inheriting Juices
    isNewSceneCreated = true;
    const newId = `scene-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    targetScene = {
      id: newId,
      name: trimmedName,
      juices: inheritJuices(currentScene.juices),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    scenes.push(targetScene);
  }

  // Add character to target scene members
  if (!targetScene.members.includes(characterId)) {
    targetScene.members.push(characterId);
  }

  return {
    updatedScenes: scenes,
    targetScene,
    isNewSceneCreated,
  };
}

// ---------------------------------------------------------------------------
// Scene state keys: name, background image and dimensions (Issue #159)
// ---------------------------------------------------------------------------

/** Scene-state key carrying the human-readable scene name. */
export const SCENE_NAME_STATE_KEY = 'std::name';
/** Scene-state key carrying `{ tinId, mimeType }` of the background image. */
export const SCENE_BACKGROUND_STATE_KEY = 'std::b-img';
/** Scene-state key carrying the configured background width in pixels. */
export const SCENE_BACKGROUND_WIDTH_STATE_KEY = 'std::b-img::width';
/** Scene-state key carrying the configured background height in pixels. */
export const SCENE_BACKGROUND_HEIGHT_STATE_KEY = 'std::b-img::height';

/** Tin label of the background image bytes (`alhalte/argenterie/bg-image`). */
export const LABEL_SCENE_BACKGROUND: bigint = labelHash('alhalte/argenterie/bg-image');

/** 64 lowercase hexadecimal characters (a 32-byte cellar tin id). */
const TIN_ID_HEX_PATTERN = /^[0-9a-f]{64}$/;

/** Parsed `std::b-img` value. */
export interface SceneBackgroundState {
  /** 64-character lowercase hex cellar tin id. */
  tinId: string;
  /** MIME type recorded at upload time (`''` for the legacy hex-string form). */
  mimeType: string;
}

/**
 * Parses a `std::b-img` state value.
 *
 * The current format is a small JSON Tin `{ tinId, mimeType }`; the legacy
 * format was a bare 64-hex string. Anything else is rejected so a malformed
 * value never reaches `juices.bgImage`.
 */
export function parseSceneBackgroundState(value: unknown): SceneBackgroundState | null {
  if (typeof value === 'string') {
    const tinId = value.trim().toLowerCase();
    return TIN_ID_HEX_PATTERN.test(tinId) ? { tinId, mimeType: '' } : null;
  }
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    const rawTinId = typeof record.tinId === 'string' ? record.tinId : '';
    const tinId = rawTinId.trim().toLowerCase();
    if (!TIN_ID_HEX_PATTERN.test(tinId)) return null;
    const mimeType = typeof record.mimeType === 'string' ? record.mimeType : '';
    return { tinId, mimeType };
  }
  return null;
}

// ---------------------------------------------------------------------------
// BGM state schema (Issue #164)
// ---------------------------------------------------------------------------

/**
 * MIME type used when a BGM track reference does not carry one.
 *
 * The legacy `std::bgm<N>` format was a bare 64-hex tin id, so old rooms have
 * no recorded MIME type. `audio/mpeg` is accepted by every browser and is only
 * a hint for the `Blob` handed to the audio element.
 */
export const BGM_AUDIO_MIME_FALLBACK = 'audio/mpeg';

/** `std::bgm` (slot 1) / `std::bgm<N>` (slot N >= 2) track reference key. */
export function bgmTrackStateKey(slot: number): string {
  return slot === 1 ? 'std::bgm' : `std::bgm${slot}`;
}

/** `bgm::<slot>::volume` state key. */
export function bgmVolumeStateKey(slot: number): string {
  return `bgm::${slot}::volume`;
}

/** `bgm::<slot>::playing` state key (Issue #164). */
export function bgmPlayingStateKey(slot: number): string {
  return `bgm::${slot}::playing`;
}

/** Decoded `std::bgm<N>` value. */
export interface BgmTrackState {
  /** 64-character lowercase hex cellar tin id. */
  tinId: string;
  /** `audio/*` MIME type, or {@link BGM_AUDIO_MIME_FALLBACK} for legacy values. */
  mimeType: string;
  /** Display name recorded at selection time (`''` when unknown). */
  name: string;
}

/** Normalizes an audio MIME type, falling back to {@link BGM_AUDIO_MIME_FALLBACK}. */
function normalizeAudioMimeType(value: unknown): string {
  return typeof value === 'string' && value.trim().toLowerCase().startsWith('audio/')
    ? value.trim().toLowerCase()
    : BGM_AUDIO_MIME_FALLBACK;
}

/**
 * Parses a `std::bgm<N>` state value.
 *
 * The current format is a small JSON Tin `{ tinId, mimeType, name }`; the
 * legacy format was a bare 64-hex string and is accepted with the
 * {@link BGM_AUDIO_MIME_FALLBACK} MIME type. Anything else is rejected so a
 * malformed value never reaches the audio manager.
 */
export function parseBgmTrackState(value: unknown): BgmTrackState | null {
  if (typeof value === 'string') {
    const tinId = value.trim().toLowerCase();
    if (!TIN_ID_HEX_PATTERN.test(tinId)) return null;
    return { tinId, mimeType: BGM_AUDIO_MIME_FALLBACK, name: '' };
  }
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    const rawTinId = typeof record.tinId === 'string' ? record.tinId : '';
    const tinId = rawTinId.trim().toLowerCase();
    if (!TIN_ID_HEX_PATTERN.test(tinId)) return null;
    return {
      tinId,
      mimeType: normalizeAudioMimeType(record.mimeType),
      name: typeof record.name === 'string' ? record.name : '',
    };
  }
  return null;
}

/**
 * Serializes the selected track of a slot into the `std::bgm<N>` JSON shape.
 *
 * Returns `null` when the slot has no valid tin id (stopped / empty slot), so
 * the caller deletes the state key instead of writing an empty reference.
 */
export function buildBgmTrackState(
  slot: Pick<BgmSlot, 'tinId' | 'mimeType' | 'name'>,
): BgmTrackState | null {
  const raw = typeof slot.tinId === 'string' ? slot.tinId.trim().toLowerCase() : '';
  if (!TIN_ID_HEX_PATTERN.test(raw)) return null;
  return {
    tinId: raw,
    mimeType: normalizeAudioMimeType(slot.mimeType),
    name: typeof slot.name === 'string' ? slot.name : '',
  };
}

/** Minimal image asset shape needed to publish a background. */
export interface SceneBackgroundAsset {
  dataUrl?: string;
  mimeType?: string;
  tinId?: string | null;
}

/** Minimal `RoomSocket` surface used to publish scene state. */
export interface SceneStateWriter {
  requestUpdateState(
    sceneId: string,
    key: string,
    value: unknown,
    timeoutMs?: number,
  ): Promise<unknown>;
  requestDeleteState(sceneId: string, key: string, timeoutMs?: number): Promise<unknown>;
}

/** Result of {@link uploadSceneBackground}. */
export interface SceneBackgroundUploadResult {
  /** Stored cellar tin id. */
  tinId: string;
  /** MIME type recorded in the state. */
  mimeType: string;
  /** Data URL of the uploaded image (used for the local preview). */
  dataUrl: string;
}

/** Decodes a `data:` URL into bytes and its MIME type. */
function decodeDataUrl(dataUrl: string): { bytes: Uint8Array; mimeType: string } | null {
  if (typeof dataUrl !== 'string' || !dataUrl.startsWith('data:')) return null;
  const comma = dataUrl.indexOf(',');
  if (comma < 0) return null;
  const meta = dataUrl.slice('data:'.length, comma);
  const payload = dataUrl.slice(comma + 1);
  const mimeType = meta.split(';')[0] || 'application/octet-stream';
  try {
    if (meta.includes(';base64')) {
      const binary = atob(payload);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i += 1) {
        bytes[i] = binary.charCodeAt(i);
      }
      return { bytes, mimeType };
    }
    return { bytes: new TextEncoder().encode(decodeURIComponent(payload)), mimeType };
  } catch {
    return null;
  }
}

/**
 * Builds a data URL from downloaded tin bytes.
 *
 * `/api/cellar/tin/<id>` returns the decrypted payload without a content type,
 * so the MIME type recorded in `std::b-img` is reused (with a PNG fallback).
 */
export function sceneBackgroundDataUrl(
  payload: Uint8Array | number[],
  mimeType: string,
): string {
  const bytes = payload instanceof Uint8Array ? payload : Uint8Array.from(payload);
  let binary = '';
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }
  const resolvedMime = mimeType && mimeType.startsWith('image/') ? mimeType : 'image/png';
  return `data:${resolvedMime};base64,${btoa(binary)}`;
}

/**
 * Downloads and decrypts a background image into a data URL.
 *
 * Throws when the room key is missing, the tin cannot be fetched, or the
 * stored MIME type is absent and the payload is not an image (the caller shows
 * the error and keeps the previous background).
 */
export async function downloadSceneBackground(
  state: SceneBackgroundState,
  roomKey: Uint8Array,
  options?: { timeoutMs?: number; fetchFn?: typeof fetch; signal?: AbortSignal },
): Promise<string> {
  if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
    throw new Error('room key is missing; the background image cannot be decrypted');
  }
  const tin = await downloadTin(state.tinId, roomKey, options);
  return sceneBackgroundDataUrl(tin.payload, state.mimeType);
}

/** Options for {@link uploadSceneBackground}. */
export interface UploadSceneBackgroundOptions {
  sceneId: string;
  /** Data URL of the picked image. */
  dataUrl?: string;
  /** Picked asset (its MIME type / fallback data URL are used when present). */
  asset?: SceneBackgroundAsset;
  /** 32-byte room key. */
  roomKey: Uint8Array | null;
  /** Connected socket used to publish `std::b-img*`. */
  roomSocket: SceneStateWriter | null;
  /** Optional configured dimensions to persist alongside the image. */
  width?: number;
  height?: number;
  timeoutMs?: number;
}

/**
 * Uploads the background image to cellar and publishes the scene state.
 *
 * Order: encode + upload the image with the room key, then write
 * `std::b-img::width` / `::height`, then `std::b-img` as
 * `{ tinId, mimeType }`. The local model is only updated by the caller after
 * this Promise resolves; any failed step rejects so nothing is committed.
 */
export async function uploadSceneBackground(
  options: UploadSceneBackgroundOptions,
): Promise<SceneBackgroundUploadResult> {
  const { sceneId, roomKey, roomSocket } = options;
  if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
    throw new Error('room key is missing; the background image cannot be encrypted');
  }
  if (!roomSocket) {
    throw new Error('room socket is unavailable; the background image was not published');
  }

  const sourceDataUrl =
    options.dataUrl ?? (options.asset?.dataUrl as string | undefined) ?? undefined;
  const decoded = sourceDataUrl ? decodeDataUrl(sourceDataUrl) : null;
  if (!decoded || decoded.bytes.length === 0) {
    throw new Error('the selected image has no readable data URL');
  }
  const mimeType = options.asset?.mimeType || decoded.mimeType || 'image/png';

  const encoded = await encodeRoomTin(decoded.bytes, LABEL_SCENE_BACKGROUND, roomKey);
  const uploaded = await uploadTin(encoded.wireBytes, {
    tinId: encoded.tinIdHex,
    ...(options.timeoutMs !== undefined ? { timeoutMs: options.timeoutMs } : {}),
  });
  const tinId = uploaded.tinId.toLowerCase();

  if (!TIN_ID_HEX_PATTERN.test(tinId)) {
    throw new Error('the uploaded background image did not produce a valid tin id');
  }

  // Dimensions first, the image reference last: a failed dimension write leaves
  // no state that points at a half-configured background.
  const widthKey = SCENE_BACKGROUND_WIDTH_STATE_KEY;
  const heightKey = SCENE_BACKGROUND_HEIGHT_STATE_KEY;
  if (typeof options.width === 'number' && options.width > 0) {
    await roomSocket.requestUpdateState(sceneId, widthKey, options.width, options.timeoutMs);
  } else {
    await roomSocket.requestDeleteState(sceneId, widthKey, options.timeoutMs);
  }
  if (typeof options.height === 'number' && options.height > 0) {
    await roomSocket.requestUpdateState(sceneId, heightKey, options.height, options.timeoutMs);
  } else {
    await roomSocket.requestDeleteState(sceneId, heightKey, options.timeoutMs);
  }

  await roomSocket.requestUpdateState(
    sceneId,
    SCENE_BACKGROUND_STATE_KEY,
    { tinId, mimeType },
    options.timeoutMs,
  );

  return { tinId, mimeType, dataUrl: sourceDataUrl as string };
}

/**
 * Clears the background image reference after the server acknowledged the
 * delete. The local model is updated by the caller.
 */
export async function clearSceneBackground(options: {
  sceneId: string;
  roomSocket: SceneStateWriter | null;
  timeoutMs?: number;
}): Promise<void> {
  if (!options.roomSocket) {
    throw new Error('room socket is unavailable; the background image was not removed');
  }
  await options.roomSocket.requestDeleteState(
    options.sceneId,
    SCENE_BACKGROUND_STATE_KEY,
    options.timeoutMs,
  );
}
