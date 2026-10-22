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
 *
 * A scene starts without any BGM row (Issue #249): a row only exists once its
 * slot carries a track. The legacy `bgm1..3` / `bgm1Volume..` fields stay for
 * compatibility with old cached scenes and old rooms.
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
    bgmSlots: [],
  };
}

/**
 * Returns the slot numbers explicitly removed from the UI (Issue #226).
 *
 * The list mirrors the server-authoritative `bgm::<slot>::removed` markers;
 * malformed entries are ignored so a bad cached value cannot hide a slot
 * twice or hide slot 0.
 */
export function getBgmRemovedSlots(juices: Juices): number[] {
  if (!Array.isArray(juices.bgmRemovedSlots)) return [];
  const slots: number[] = [];
  for (const value of juices.bgmRemovedSlots) {
    const slot = typeof value === 'number' ? value : Number(value);
    if (Number.isInteger(slot) && slot >= 1 && !slots.includes(slot)) {
      slots.push(slot);
    }
  }
  return slots;
}

/** True when `slot` carries a removal marker (Issue #226). */
export function isBgmSlotRemoved(juices: Juices, slot: number): boolean {
  return getBgmRemovedSlots(juices).includes(slot);
}

/**
 * Adds / removes a slot from the local removal marker list (Issue #226).
 *
 * The caller is responsible for writing the `bgm::<slot>::removed` state key;
 * this only keeps the scene model in sync with it.
 */
export function setBgmSlotRemoved(
  juices: Juices,
  slot: number,
  removed: boolean,
): void {
  if (!Number.isInteger(slot) || slot < 1) return;
  const slots = new Set(getBgmRemovedSlots(juices));
  if (removed) {
    slots.add(slot);
  } else {
    slots.delete(slot);
  }
  juices.bgmRemovedSlots = [...slots].sort((a, b) => a - b);
}

/**
 * True when a slot carries a track reference (Issue #249).
 *
 * A row exists iff its slot has a valid tin id: a paused track still has one,
 * a cleared / never selected slot does not. The check is intentionally a
 * non-empty string test, not a 64-hex test, so a legacy note-like value
 * (`bgm1: 'march.ogg'` in an old cached scene) still renders a row; the state
 * parsers ([`parseBgmTrackState`]) are the strict gate for playback.
 */
export function hasBgmTrack(slot: Pick<BgmSlot, 'tinId'>): boolean {
  return typeof slot.tinId === 'string' && slot.tinId.trim().length > 0;
}

/** Default volume of a BGM slot without a `bgm::<slot>::volume` value. */
const BGM_DEFAULT_VOLUME = 0.8;

/**
 * Normalizes a BGM transport timestamp (UNIX milliseconds, Issue #250).
 *
 * Returns `undefined` for anything that is not a finite number, so a malformed
 * cached / broadcast value behaves exactly like a legacy room without the key.
 */
function normalizeBgmTimestamp(value: unknown): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value)) return undefined;
  return Math.max(0, Math.trunc(value));
}

/** Rows derived from the legacy `bgm1..3` fields of a scene without `bgmSlots`. */
function legacyBgmSlots(juices: Juices): BgmSlot[] {
  const row = (slot: number, tinId: string | undefined, volume?: number): BgmSlot => ({
    slot,
    tinId: tinId ?? null,
    mimeType: BGM_AUDIO_MIME_FALLBACK,
    volume: typeof volume === 'number' ? volume : BGM_DEFAULT_VOLUME,
    // The legacy schema had no playback intent: a stored track was playing.
    isPlaying: true,
  });
  return [
    row(1, juices.bgm1, juices.bgm1Volume),
    row(2, juices.bgm2, juices.bgm2Volume),
    row(3, juices.bgm3, juices.bgm3Volume),
  ];
}

/**
 * Returns the BGM rows of a scene: **one row per slot that has a track**
 * (Issue #249).
 *
 * - An explicitly present `bgmSlots` array is authoritative: rows without a
 *   valid tin id are dropped, so an untouched scene has no rows and a scene
 *   whose list is `[]` shows nothing even when the legacy `bgm1..3` mirrors
 *   linger. Only an absent field falls back to the legacy mirrors, and then
 *   only the slots whose legacy field is set are returned (no synthetic
 *   `1..3` null rows).
 * - Slots carrying a `bgm::<slot>::removed` marker are always filtered out
 *   (Issue #226 tombstone), regardless of any stale track value.
 * - The result is sorted by slot and never contains a duplicate slot number.
 */
export function getNormalizedBgmSlots(juices: Juices): BgmSlot[] {
  const removed = new Set(getBgmRemovedSlots(juices));
  const candidates: BgmSlot[] = Array.isArray(juices.bgmSlots)
    ? juices.bgmSlots
    : legacyBgmSlots(juices);

  const bySlot = new Map<number, BgmSlot>();
  for (const slot of candidates) {
    if (!slot || typeof slot !== 'object') continue;
    const slotNum = typeof slot.slot === 'number' ? slot.slot : Number(slot.slot);
    if (!Number.isInteger(slotNum) || slotNum < 1) continue;
    if (!hasBgmTrack(slot)) continue;
    if (removed.has(slotNum)) continue;
    // A duplicated slot number is a malformed cached value: the first entry wins.
    if (bySlot.has(slotNum)) continue;
    const volume =
      typeof slot.volume === 'number' && Number.isFinite(slot.volume)
        ? slot.volume
        : BGM_DEFAULT_VOLUME;
    // Transport timestamps are UNIX milliseconds (Issue #250). A malformed
    // cached value is dropped so a NaN can never reach the offset arithmetic.
    bySlot.set(slotNum, {
      ...slot,
      slot: slotNum,
      volume,
      isPlaying: Boolean(slot.isPlaying),
      startedAt: normalizeBgmTimestamp(slot.startedAt),
      pausedAt: normalizeBgmTimestamp(slot.pausedAt),
    });
  }
  return [...bySlot.values()].sort((a, b) => a.slot - b.slot);
}

/**
 * Inherits Juices from a parent / source scene.
 *
 * Used by the new-scene paths that copy the current scene's background / BGM
 * (`SceneSettingsTab`'s 「Juicesを継承」 toggle and the implicit creation of a
 * move destination in {@link moveMemberToScene}). The background dimensions and
 * the BGM transport follow `createDefaultJuices` when absent; `bgmRemovedSlots`
 * is deliberately not inherited.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### scene の管理操作`
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
  if (Array.isArray(source.bgmSlots)) {
    // Only rows that carry a track are inherited (Issue #249); the removal
    // markers themselves are scene-specific (see below).
    result.bgmSlots = getNormalizedBgmSlots(source).map((s) => ({ ...s }));
  }
  // `bgmRemovedSlots` is deliberately not inherited: it is scene-specific
  // server state, while the inherited `bgmSlots` list already omits deleted
  // rows. Copying the markers would make the new scene hide slots that have
  // no server marker of their own.
  return result;
}

export {
  calculateBackgroundDimensions,
  calculateDimensionsAndCrop,
  type BackgroundDimensionParams,
  type BackgroundDimensionResult,
} from './backgroundDimension.js';


/**
 * Generates a client-side scene id for a `CreateScene` request (Issue #269).
 *
 * The server never allocates scene ids (`CreateScene` carries one), so the
 * caller has to pick it. The shape is the historical
 * `scene-<unix ms>-<4 base36 chars>`, which is unique enough per room and
 * readable in logs; every creation path (header switcher, move modal, scene
 * creation from the settings tab, implicit move destination) shares this
 * helper so the format cannot drift.
 */
export function generateSceneId(): string {
  return `scene-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
}

/**
 * Moves a member to target scene. If the target scene name does not exist,
 * automatically creates the scene inheriting Juices from the current scene.
 *
 * `targetSceneId` lets the caller pin the id of the scene it creates (Issue
 * #269): the move modal generates the id up front so the `CreateScene` ack and
 * the following `MoveCharacter` request address the same scene. Without it a
 * fresh id is generated with {@link generateSceneId}. The id is ignored when
 * the name already matches an existing scene (no scene is created).
 */
export function moveMemberToScene(
  characterId: string,
  targetSceneName: string,
  currentScene: Scene,
  existingScenes: Scene[],
  targetSceneId?: string,
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
    const newId = targetSceneId?.trim() || generateSceneId();
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

/**
 * `bgm::<slot>::removed` state key (Issue #226).
 *
 * A slot deleted in the settings tab is recorded as `true` on the server. The
 * track / playing / volume keys alone cannot distinguish "deleted" from the
 * AssetPicker's NONE (which keeps the row), so this marker is the persistent,
 * server-authoritative signal that hides the slot on every client and reload.
 */
export function bgmRemovedStateKey(slot: number): string {
  return `bgm::${slot}::removed`;
}

/**
 * `bgm::<slot>::started_at` state key (Issue #250).
 *
 * A UNIX millisecond timestamp of when the current continuous playback started.
 * The playback position of a playing slot is `now - started_at` (modulo the
 * track duration), so a client that joins mid-track seeks to the same offset and
 * ⏪ (restart) just rewrites the timestamp to `now` for every client.
 */
export function bgmStartedAtStateKey(slot: number): string {
  return `bgm::${slot}::started_at`;
}

/**
 * `bgm::<slot>::paused_at` state key (Issue #250).
 *
 * A UNIX millisecond timestamp of when the slot was paused; it is deleted while
 * playing. A paused slot keeps its position as `paused_at - started_at`, and
 * resuming rewrites `started_at` to `now - (paused_at - started_at)` before
 * deleting this key.
 */
export function bgmPausedAtStateKey(slot: number): string {
  return `bgm::${slot}::paused_at`;
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
