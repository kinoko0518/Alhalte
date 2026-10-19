/**
 * WebSocket synchronization client for Alhalte room communication.
 *
 * Implements real-time bidirectional synchronization between @alhalte/argenterie
 * and alhalte-banquette / alhalte-maitre-d using ClientFrame and ServerFrame protocols.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### protocol`
 * - ARCHITECTURE.md: `## alhalte-maitre-d -> ### WebSocket ルーティング`
 * - GitHub Issue #56: `feat(argenterie): WebSocket によるサーバー操作同期 (ClientFrame / ServerFrame)`
 */

import {
  LABEL_ALHALTE_TIN_ID,
  LABEL_ALHALTE_UNIX_TIME,
  LABEL_NEED_NOT_KNOW,
  LABEL_PRIMITIVE_BOOL,
  LABEL_PRIMITIVE_STRING,
  LABEL_PRIMITIVE_U8,
  LABEL_PRIMITIVE_U16,
  LABEL_PRIMITIVE_U32,
  LABEL_PRIMITIVE_U64,
  bytesToHex,
  labelHash,
} from '@alhalte/pantry';
import type { Character, LogRecord, LogSystemEvent } from '../types/index.js';
import {
  connectionManager as defaultConnectionManager,
  ConnectionManager,
} from './connection.js';
import { ChunkLogManager, contentTinToBytes, parseLogRecord } from './log.js';

/**
 * Self-typed Tin container JSON representation.
 *
 * The Rust `Tin` struct is `{ label: u64, payload: Vec<u8> }`, so the JSON wire
 * form is exactly `{ label: number, payload: number[] }`.
 */
export interface TinJson {
  label: number;
  payload: number[];
}

// =========================================================================
// Operation errors & timeout
// =========================================================================

/**
 * Default time in milliseconds to wait for a server acknowledgement before
 * rejecting a Promise-based operation with [`OperationTimeoutError`].
 */
export const DEFAULT_OPERATION_TIMEOUT_MS = 8000;

/**
 * Error used when a Promise-based operation receives no acknowledgement in time.
 *
 * The `code` is always `'timeout'` so callers can branch on it uniformly with
 * [`ServerOperationError`].
 */
export class OperationTimeoutError extends Error {
  /** Machine-readable error code (`'timeout'`). */
  public readonly code = 'timeout';
  /** Operation kind that timed out (e.g. `'appendLog'`). */
  public readonly operation: string;
  /** Timeout applied to the operation, in milliseconds. */
  public readonly timeoutMs: number;

  constructor(operation: string, timeoutMs: number) {
    super(`Operation '${operation}' timed out after ${timeoutMs}ms`);
    this.name = 'OperationTimeoutError';
    this.operation = operation;
    this.timeoutMs = timeoutMs;
  }
}

/**
 * Error used when the server rejects a Promise-based operation.
 *
 * `code` is the `ServerFrame::Error` code (e.g. `'AppendLogFailed'`,
 * `'PermissionDenied'`, or `'disconnected'` when the socket closed before the
 * acknowledgement arrived).
 */
export class ServerOperationError extends Error {
  /** Machine-readable error code reported by the server. */
  public readonly code: string;
  /** Optional structured details attached by the client. */
  public readonly details?: unknown;

  constructor(code: string, message?: string, details?: unknown) {
    super(message && message.length > 0 ? message : `Server operation failed: ${code}`);
    this.name = 'ServerOperationError';
    this.code = code;
    this.details = details;
  }
}

// =========================================================================
// Tin labels used by the client
// =========================================================================

/** Label of a chat log entry JSON payload (`alhalte/argenterie/log-record`). */
export const LABEL_LOG_RECORD = Number(labelHash('alhalte/argenterie/log-record'));

/** Label of an arbitrary JSON payload (`alhalte/argenterie/json`), used for objects and arrays. */
export const LABEL_JSON = Number(labelHash('alhalte/argenterie/json'));

/** Label of a 32-bit float payload (`primitive/f32`). */
export const LABEL_PRIMITIVE_F32 = Number(labelHash('primitive/f32'));

/** Label of a 64-bit float payload (`primitive/f64`). */
export const LABEL_PRIMITIVE_F64 = Number(labelHash('primitive/f64'));

/** Label of a BGM change log event (`alhalte/banquette/log/change-bgm`). */
export const LABEL_BANQUETTE_LOG_CHANGE_BGM = Number(
  labelHash('alhalte/banquette/log/change-bgm'),
);

/** Label of a background image change log event (`alhalte/banquette/log/change-bg-img`). */
export const LABEL_BANQUETTE_LOG_CHANGE_BG_IMG = Number(
  labelHash('alhalte/banquette/log/change-bg-img'),
);

/** Label of a character entrance log event (`alhalte/banquette/log/character-came`). */
export const LABEL_BANQUETTE_LOG_CHARACTER_CAME = Number(
  labelHash('alhalte/banquette/log/character-came'),
);

/** Label of a character departure log event (`alhalte/banquette/log/character-gone`). */
export const LABEL_BANQUETTE_LOG_CHARACTER_GONE = Number(
  labelHash('alhalte/banquette/log/character-gone'),
);

/** Acknowledgement payload of an appended log entry (`LogEventAppended`). */
export interface LogAppendedAck {
  scene_id: string;
  chunk_index: number;
  position: number;
  entry: any;
}

/** Acknowledgement payload of `StateUpdated`. */
export interface StateUpdatedAck {
  scene_id: string;
  key: string;
  value: any;
}

/** Acknowledgement payload of `StateDeleted`. */
export interface StateDeletedAck {
  scene_id: string;
  key: string;
}

/** Acknowledgement payload of `SceneCreated` / `SceneDeleted`. */
export interface SceneAck {
  scene_id: string;
}

/** Acknowledgement payload of `CharacterMoved`. */
export interface CharacterMovedAck {
  character_id: number;
  to_scene_id: string;
}

/** Acknowledgement payload of `CharacterCreated` / `CharacterUpdated` / `CharacterDeleted`. */
export interface CharacterAck {
  character_id: number;
}

/** Acknowledgement payload of `LogEventRewound`. */
export interface LogRewoundAck {
  scene_id: string;
  chunk_index: number;
  position: number;
}

/** Acknowledgement payload of `DoneUpdated`. */
export interface DoneUpdatedAck {
  done: boolean;
}

// =========================================================================
// Tin codec
// =========================================================================

/** Payload bytes viewed as a plain `number[]` (the JSON wire representation). */
function toByteArray(payload: unknown): number[] {
  if (Array.isArray(payload)) {
    return payload.map((byte) => Number(byte) & 0xff);
  }
  if (payload instanceof Uint8Array) {
    return Array.from(payload);
  }
  if (payload && typeof payload === 'object') {
    return Object.values(payload as Record<string, number>).map((byte) => Number(byte) & 0xff);
  }
  return [];
}

/**
 * Returns true when `value` already looks like a self-typed Tin container.
 */
export function isTinJson(value: unknown): value is TinJson {
  if (!value || typeof value !== 'object') return false;
  const label = (value as TinJson).label;
  return (
    (typeof label === 'number' || typeof label === 'bigint') &&
    'payload' in (value as Record<string, unknown>)
  );
}

/**
 * Writes an unsigned integer as little-endian bytes.
 */
function writeLeBytes(value: bigint, byteLength: number): number[] {
  const bytes: number[] = [];
  let rest = value < 0n ? 0n : value;
  for (let i = 0; i < byteLength; i++) {
    bytes.push(Number(rest & 0xffn));
    rest >>= 8n;
  }
  return bytes;
}

/**
 * Reads an unsigned little-endian integer from payload bytes.
 */
function readLeBytes(payload: number[], byteLength: number): bigint {
  let value = 0n;
  for (let i = byteLength - 1; i >= 0; i--) {
    value = (value << 8n) | BigInt(payload[i] ?? 0);
  }
  return value;
}

/**
 * Reads an unsigned little-endian integer as a `number`, or as a `bigint` when
 * it exceeds `Number.MAX_SAFE_INTEGER`.
 */
function readUnsigned(payload: number[], byteLength: number): number | bigint {
  const value = readLeBytes(payload, byteLength);
  return value > BigInt(Number.MAX_SAFE_INTEGER) ? value : Number(value);
}

/**
 * Normalizes any value into a Tin container.
 *
 * - An existing Tin is returned normalized (payload as `number[]`, label as `number`).
 * - `string` -> `primitive/string`
 * - `number` -> `primitive/u64` when a non-negative integer, `primitive/f64` otherwise
 * - `bigint` -> `primitive/u64`
 * - `boolean` -> `primitive/bool`
 * - `null` / `undefined` -> `need not know` with an empty payload
 * - arrays and objects -> JSON text with `alhalte/argenterie/json`
 *
 * Passing an explicit `label` overrides the type-derived label.
 *
 * Note: JSON cannot represent a full 64-bit label without loss, so labels are
 * forwarded as JavaScript numbers. The server does not validate `label` values
 * for log entries and state values, and `Number(labelHash(...))` reproduces the
 * same double on both sides, so round-trips stay stable.
 */
export function jsonToTin(value: unknown, label?: number | bigint): TinJson {
  if (isTinJson(value)) {
    return { label: Number(value.label), payload: toByteArray(value.payload) };
  }
  const explicit = label === undefined || label === null ? undefined : Number(label);

  if (value === null || value === undefined) {
    return { label: explicit ?? Number(LABEL_NEED_NOT_KNOW), payload: [] };
  }
  if (typeof value === 'string') {
    return {
      label: explicit ?? Number(LABEL_PRIMITIVE_STRING),
      payload: Array.from(new TextEncoder().encode(value)),
    };
  }
  if (typeof value === 'boolean') {
    return { label: explicit ?? Number(LABEL_PRIMITIVE_BOOL), payload: [value ? 1 : 0] };
  }
  if (typeof value === 'bigint') {
    return { label: explicit ?? Number(LABEL_PRIMITIVE_U64), payload: writeLeBytes(value, 8) };
  }
  if (typeof value === 'number') {
    if (Number.isInteger(value) && value >= 0) {
      return {
        label: explicit ?? Number(LABEL_PRIMITIVE_U64),
        payload: writeLeBytes(BigInt(value), 8),
      };
    }
    const buffer = new Uint8Array(8);
    new DataView(buffer.buffer).setFloat64(0, value, true);
    return { label: explicit ?? LABEL_PRIMITIVE_F64, payload: Array.from(buffer) };
  }

  return {
    label: explicit ?? LABEL_JSON,
    payload: Array.from(new TextEncoder().encode(JSON.stringify(value))),
  };
}

/**
 * Decodes a Tin container back into a JavaScript value.
 *
 * Primitive labels are decoded to their natural type, JSON payloads are parsed,
 * and anything undecodable falls back to the payload's UTF-8 text. Non-Tin
 * values are returned unchanged so the function is safe on duck-typed input.
 */
export function tinToJson(tin: unknown): any {
  if (!isTinJson(tin)) return tin;
  const label = Number(tin.label);
  const payload = toByteArray(tin.payload);
  const bytes = Uint8Array.from(payload);

  if (label === Number(LABEL_PRIMITIVE_STRING)) {
    return new TextDecoder().decode(bytes);
  }
  if (label === Number(LABEL_PRIMITIVE_BOOL)) {
    return (payload[0] ?? 0) !== 0;
  }
  if (label === Number(LABEL_PRIMITIVE_U8) && payload.length === 1) {
    return readUnsigned(payload, 1);
  }
  if (label === Number(LABEL_PRIMITIVE_U16) && payload.length === 2) {
    return readUnsigned(payload, 2);
  }
  if (label === Number(LABEL_PRIMITIVE_U32) && payload.length === 4) {
    return readUnsigned(payload, 4);
  }
  if (label === Number(LABEL_PRIMITIVE_U64) && payload.length === 8) {
    return readUnsigned(payload, 8);
  }
  if (label === LABEL_PRIMITIVE_F32 && payload.length === 4) {
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat32(0, true);
  }
  if (label === LABEL_PRIMITIVE_F64 && payload.length === 8) {
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getFloat64(0, true);
  }
  if (label === Number(LABEL_ALHALTE_TIN_ID) && payload.length === 32) {
    return bytesToHex(bytes);
  }
  if (label === Number(LABEL_ALHALTE_UNIX_TIME) && payload.length === 8) {
    return Number(readLeBytes(payload, 8));
  }

  const text = new TextDecoder().decode(bytes);
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/** Cursor over postcard payload bytes used by the system-event decoders. */
interface PostcardCursor {
  bytes: number[];
  offset: number;
}

/**
 * Reads an unsigned Postcard varint (u64) as a `bigint`.
 *
 * Returns `null` on truncation or an overlong (more than 64-bit) encoding so the
 * caller can fall back to a text row instead of throwing.
 */
function readPostcardVarint(cursor: PostcardCursor): bigint | null {
  let value = 0n;
  let shift = 0n;
  while (cursor.offset < cursor.bytes.length) {
    const byte = cursor.bytes[cursor.offset++];
    value |= BigInt(byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) {
      return BigInt.asUintN(64, value);
    }
    shift += 7n;
    if (shift > 63n) return null;
  }
  return null;
}

/** Reads a Postcard `Option` discriminant (0 = none, 1 = some). */
function readPostcardOptionTag(cursor: PostcardCursor): boolean | null {
  if (cursor.offset >= cursor.bytes.length) return null;
  const tag = cursor.bytes[cursor.offset++];
  if (tag === 0) return false;
  if (tag === 1) return true;
  return null;
}

/** Reads a fixed-size byte string (e.g. a 32-byte `TinId`) as lowercase hex. */
function readPostcardFixedHex(cursor: PostcardCursor, length: number): string | null {
  if (cursor.bytes.length < cursor.offset + length) return null;
  const slice = Uint8Array.from(cursor.bytes.slice(cursor.offset, cursor.offset + length));
  cursor.offset += length;
  return bytesToHex(slice);
}

/** Reads a little-endian IEEE 754 `f32`. */
function readPostcardF32(cursor: PostcardCursor): number | null {
  if (cursor.bytes.length < cursor.offset + 4) return null;
  const bytes = Uint8Array.from(cursor.bytes.slice(cursor.offset, cursor.offset + 4));
  cursor.offset += 4;
  return new DataView(bytes.buffer, bytes.byteOffset, 4).getFloat32(0, true);
}

/**
 * Decodes a banquette system-event Tin into a {@link LogSystemEvent}.
 *
 * Field order follows the serde definitions in `alhalte-banquette/src/log.rs`:
 * - `change-bgm`: `Option<TinId>`, `Option<f32>`, `u32`, `Option<u8>`
 * - `change-bg-img`: `Option<TinId>`, `Option<u32>`, `Option<u32>`
 * - `character-came` / `character-gone`: `u64`
 *
 * Returns `null` for unknown labels or malformed payloads; callers then fall
 * back to the text path without ever decoding the bytes as UTF-8 chat content.
 */
export function decodeLogSystemEvent(entry: unknown): LogSystemEvent | null {
  if (!isTinJson(entry)) return null;
  const label = Number(entry.label);
  const cursor: PostcardCursor = { bytes: toByteArray(entry.payload), offset: 0 };

  if (label === LABEL_BANQUETTE_LOG_CHARACTER_CAME) {
    const characterId = readPostcardVarint(cursor);
    if (characterId === null) return null;
    return { type: 'character-came', characterId: characterId.toString() };
  }
  if (label === LABEL_BANQUETTE_LOG_CHARACTER_GONE) {
    const characterId = readPostcardVarint(cursor);
    if (characterId === null) return null;
    return { type: 'character-gone', characterId: characterId.toString() };
  }
  if (label === LABEL_BANQUETTE_LOG_CHANGE_BGM) {
    const hasTinId = readPostcardOptionTag(cursor);
    if (hasTinId === null) return null;
    let bgmTinId: string | undefined;
    if (hasTinId) {
      const hex = readPostcardFixedHex(cursor, 32);
      if (hex === null) return null;
      bgmTinId = hex;
    }
    const hasVolume = readPostcardOptionTag(cursor);
    if (hasVolume === null) return null;
    let volume: number | undefined;
    if (hasVolume) {
      const value = readPostcardF32(cursor);
      if (value === null) return null;
      volume = value;
    }
    const slotValue = readPostcardVarint(cursor);
    if (slotValue === null) return null;
    const hasTrack = readPostcardOptionTag(cursor);
    if (hasTrack === null) return null;
    let trackIndex: number | undefined;
    if (hasTrack) {
      if (cursor.offset >= cursor.bytes.length) return null;
      trackIndex = cursor.bytes[cursor.offset++];
    }
    return {
      type: 'change-bgm',
      slot: Number(slotValue),
      ...(bgmTinId !== undefined ? { bgmTinId } : {}),
      ...(volume !== undefined ? { volume } : {}),
      ...(trackIndex !== undefined ? { trackIndex } : {}),
    };
  }
  if (label === LABEL_BANQUETTE_LOG_CHANGE_BG_IMG) {
    const hasTinId = readPostcardOptionTag(cursor);
    if (hasTinId === null) return null;
    let imageTinId: string | undefined;
    if (hasTinId) {
      const hex = readPostcardFixedHex(cursor, 32);
      if (hex === null) return null;
      imageTinId = hex;
    }
    const hasWidth = readPostcardOptionTag(cursor);
    if (hasWidth === null) return null;
    let width: number | undefined;
    if (hasWidth) {
      const value = readPostcardVarint(cursor);
      if (value === null) return null;
      width = Number(value);
    }
    const hasHeight = readPostcardOptionTag(cursor);
    if (hasHeight === null) return null;
    let height: number | undefined;
    if (hasHeight) {
      const value = readPostcardVarint(cursor);
      if (value === null) return null;
      height = Number(value);
    }
    return {
      type: 'change-bg-img',
      ...(imageTinId !== undefined ? { imageTinId } : {}),
      ...(width !== undefined ? { width } : {}),
      ...(height !== undefined ? { height } : {}),
    };
  }

  return null;
}

/**
 * Builds a `system` log record from a decoded banquette event Tin.
 *
 * The postcard payload is never decoded as UTF-8, so no byte garbage can leak
 * into the chat text. A known event label with a malformed payload still yields
 * a `system` record (without an `event`), which renders as a generic status row
 * instead of binary text. Returns `null` for unknown labels.
 */
function systemEventToLogRecord(
  entry: unknown,
  sceneId: string,
  chunkIndex: number,
  position: number,
): LogRecord | null {
  if (!isTinJson(entry)) return null;
  const label = Number(entry.label);
  const event = decodeLogSystemEvent(entry);
  if (!event && !isBanquetteSystemEventLabel(label)) return null;
  return parseLogRecord(
    {
      sceneId,
      content: '',
      speaker: 'System',
      timestamp: 0,
      chunkIndex,
      position,
      kind: 'system',
      ...(event ? { event } : {}),
    },
    `log-${chunkIndex}-${position}`,
  );
}

/** Returns true for the four banquette system-event labels. */
function isBanquetteSystemEventLabel(label: number): boolean {
  return (
    label === LABEL_BANQUETTE_LOG_CHANGE_BGM ||
    label === LABEL_BANQUETTE_LOG_CHANGE_BG_IMG ||
    label === LABEL_BANQUETTE_LOG_CHARACTER_CAME ||
    label === LABEL_BANQUETTE_LOG_CHARACTER_GONE
  );
}

/**
 * Encodes a `LogRecord` (or any plain record object) as a Tin whose payload is
 * UTF-8 JSON, embedding `clientOpId` so that the sender can correlate the
 * `LogEventAppended` broadcast with this append.
 */
export function logRecordToTin(
  record: LogRecord | Record<string, any> | null | undefined,
  clientOpId: string,
): TinJson {
  const payload: Record<string, any> = {};
  if (record && typeof record === 'object') {
    for (const key of [
      'id',
      'sceneId',
      'content',
      'speaker',
      'speakerId',
      'timestamp',
      'chunkId',
      'kind',
      'event',
    ]) {
      const value = (record as Record<string, any>)[key];
      if (value !== undefined) {
        payload[key] = value;
      }
    }
    // `contentTin` is the extension-pipeline result (Issue #161). Normalize the
    // accepted representations to the canonical byte array so banquette stores a
    // stable JSON payload and kitchen can decode it.
    const contentTinBytes = contentTinToBytes((record as Record<string, any>).contentTin);
    if (contentTinBytes) {
      payload.contentTin = Array.from(contentTinBytes);
    }
  }
  payload.clientOpId = clientOpId;
  return {
    label: LABEL_LOG_RECORD,
    payload: Array.from(new TextEncoder().encode(JSON.stringify(payload))),
  };
}

/**
 * Decodes a log entry Tin into a `LogRecord`.
 *
 * Returns `null` for tombstone (`null`) entries. The position is used for the
 * fallback record ID when the payload does not carry one.
 */
export function tinToLogRecord(
  entry: unknown,
  sceneId: string,
  chunkIndex = 0,
  position = 0,
): LogRecord | null {
  if (entry === null || entry === undefined) return null;

  // Banquette system events are postcard payloads: decode them by label before
  // any UTF-8 fallback so they are never shown as binary chat text.
  const systemRecord = systemEventToLogRecord(entry, sceneId, chunkIndex, position);
  if (systemRecord) return systemRecord;

  let raw: Record<string, any>;
  if (isTinJson(entry)) {
    const decoded = tinToJson(entry);
    raw =
      decoded && typeof decoded === 'object' && !Array.isArray(decoded)
        ? (decoded as Record<string, any>)
        : { content: typeof decoded === 'string' ? decoded : '' };
  } else if (typeof entry === 'object') {
    raw = entry as Record<string, any>;
  } else {
    raw = { content: String(entry) };
  }

  return parseLogRecord(
    { ...raw, sceneId: raw.sceneId || sceneId, chunkIndex, position },
    `log-${chunkIndex}-${position}`,
  );
}

/**
 * Converts a `Character` model into the `HashMap<String, Tin>` attribute map
 * expected by `ClientFrame::CreateCharacter` / `UpdateCharacter`.
 *
 * The human-readable name is stored under the reserved `std::name` key (as the
 * Rust side does); remaining model fields keep their model names. The character
 * `id` is omitted because banquette allocates character IDs.
 */
export function characterToTinMap(
  character: Character | Record<string, any> | null | undefined,
): Record<string, TinJson> {
  const source = (character ?? {}) as Record<string, any>;
  const map: Record<string, TinJson> = {};

  if (typeof source.name === 'string') {
    map['std::name'] = jsonToTin(source.name);
  }
  if (typeof source.initiative === 'number') {
    map.initiative = jsonToTin(source.initiative);
  }
  for (const key of ['iconUrl', 'color', 'description', 'reference', 'chatPalette'] as const) {
    if (typeof source[key] === 'string') {
      map[key] = jsonToTin(source[key]);
    }
  }
  for (const key of ['hideStatus', 'isNarrative', 'placedAsPiece'] as const) {
    if (typeof source[key] === 'boolean') {
      map[key] = jsonToTin(source[key]);
    }
  }
  if (Array.isArray(source.statuses)) {
    map.statuses = jsonToTin(source.statuses);
  }
  if (Array.isArray(source.parameters)) {
    map.parameters = jsonToTin(source.parameters);
  }
  if (source.piecePosition && typeof source.piecePosition === 'object') {
    map.piecePosition = jsonToTin(source.piecePosition);
  }

  return map;
}

/**
 * Rebuilds a `Character` model from a banquette character attribute map.
 *
 * Non-Tin values are accepted as-is so duck-typed server data still renders.
 */
export function tinMapToCharacter(
  id: string | number,
  map: Record<string, any> | null | undefined,
): Character {
  const source = map ?? {};
  const read = (key: string): any => {
    const value = source[key];
    if (value === undefined) return undefined;
    return isTinJson(value) ? tinToJson(value) : value;
  };

  const stdName = read('std::name');
  const name = typeof stdName === 'string' && stdName.length > 0 ? stdName : read('name');
  const initiative = read('initiative');
  const iconUrl = read('iconUrl');
  const color = read('color');
  const description = read('description');
  const reference = read('reference');
  const statuses = read('statuses');
  const parameters = read('parameters');
  const chatPalette = read('chatPalette');
  const hideStatus = read('hideStatus');
  const isNarrative = read('isNarrative');
  const placedAsPiece = read('placedAsPiece');
  const piecePosition = read('piecePosition');

  return {
    id: String(id),
    name: typeof name === 'string' && name.length > 0 ? name : `Character ${id}`,
    initiative: typeof initiative === 'number' ? initiative : 0,
    iconUrl: typeof iconUrl === 'string' ? iconUrl : undefined,
    color: typeof color === 'string' ? color : undefined,
    description: typeof description === 'string' ? description : '',
    reference: typeof reference === 'string' ? reference : undefined,
    statuses: Array.isArray(statuses) ? statuses : [],
    parameters: Array.isArray(parameters) ? parameters : [],
    chatPalette: typeof chatPalette === 'string' ? chatPalette : '',
    hideStatus: Boolean(hideStatus),
    isNarrative: isNarrative === undefined ? undefined : Boolean(isNarrative),
    placedAsPiece: placedAsPiece === undefined ? undefined : Boolean(placedAsPiece),
    piecePosition:
      piecePosition &&
      typeof piecePosition.x === 'number' &&
      typeof piecePosition.y === 'number'
        ? { x: piecePosition.x, y: piecePosition.y }
        : undefined,
  };
}

/**
 * Compares two Tin values structurally (label + payload bytes).
 */
function tinEquals(a: unknown, b: unknown): boolean {
  if (isTinJson(a) && isTinJson(b)) {
    const left = toByteArray(a.payload);
    const right = toByteArray(b.payload);
    if (Number(a.label) !== Number(b.label) || left.length !== right.length) return false;
    return left.every((byte, index) => byte === right[index]);
  }
  if (a === b) return true;
  try {
    return JSON.stringify(a) === JSON.stringify(b);
  } catch {
    return false;
  }
}

/**
 * Reads the `clientOpId` embedded in an `AppendLog` entry, if any.
 */
function readClientOpId(entry: unknown): string | null {
  if (!isTinJson(entry)) return null;
  try {
    const decoded = new TextDecoder().decode(Uint8Array.from(toByteArray(entry.payload)));
    const json = JSON.parse(decoded);
    if (json && typeof json === 'object' && typeof json.clientOpId === 'string') {
      return json.clientOpId;
    }
  } catch {
    // Not a JSON payload (e.g. a postcard-encoded log event).
  }
  return null;
}

/**
 * Decodes the leading fields of a postcard-encoded `ChangeBgmEvent`.
 *
 * Field order is `bgm_tin_id: Option<TinId>`, `volume: Option<f32>`,
 * `slot: u32` (varint), `track_index: Option<u8>`. Only the tin id and the slot
 * are needed for acknowledgement correlation.
 */
function decodeChangeBgmEntry(
  entry: unknown,
): { tinIdHex: string | null; slot: number | null } | null {
  if (!isTinJson(entry) || Number(entry.label) !== LABEL_BANQUETTE_LOG_CHANGE_BGM) {
    return null;
  }
  const payload = toByteArray(entry.payload);
  let offset = 0;

  const tinIdTag = payload[offset++];
  if (tinIdTag !== 0 && tinIdTag !== 1) return null;
  let tinIdHex: string | null = null;
  if (tinIdTag === 1) {
    if (payload.length < offset + 32) return null;
    tinIdHex = bytesToHex(Uint8Array.from(payload.slice(offset, offset + 32)));
    offset += 32;
  }

  const volumeTag = payload[offset++];
  if (volumeTag === 1) {
    if (payload.length < offset + 4) return null;
    offset += 4;
  } else if (volumeTag !== 0) {
    return null;
  }

  let slot = 0;
  let shift = 0;
  for (;;) {
    if (offset >= payload.length || shift > 28) return null;
    const byte = payload[offset++];
    slot += (byte & 0x7f) * 2 ** shift;
    shift += 7;
    if ((byte & 0x80) === 0) break;
  }

  return { tinIdHex, slot };
}

/** Summary of one scene as sent by banquette (`SceneList` / `SceneInfo`). */
export interface SceneSummary {
  /** Scene identifier. */
  id: string;
  /** Human-readable `std::name` value, falling back to the scene id. */
  name?: string;
  /** Number of present characters. */
  present_character_count?: number;
  /** Number of active connected clients. */
  active_client_count?: number;
  /** Whether the scene is currently loaded in memory. */
  is_loaded?: boolean;
  /**
   * Character IDs present in the scene, as reported by `SceneSummary`.
   *
   * Clients rebuild `scene.members` from this list on reload / reconnect.
   * Older servers omit the field; in that case the previous membership is kept.
   */
  present_characters?: number[];
}

/**
 * One `SceneList` entry.
 *
 * Older servers sent bare scene ids; the current protocol sends summaries. The
 * client accepts both so a stale server cannot break the scene switcher.
 */
export type SceneListEntry = string | SceneSummary;

/** Normalizes a `SceneList` entry to its scene id. */
export function sceneListEntryId(entry: unknown): string {
  if (typeof entry === 'string') return entry;
  if (entry && typeof entry === 'object') {
    const id = (entry as Record<string, unknown>).id;
    if (typeof id === 'string') return id;
  }
  return String(entry ?? '');
}

/** Normalizes a `SceneList` entry to a summary object. */
export function sceneListEntrySummary(entry: unknown): SceneSummary | null {
  if (typeof entry === 'string') return { id: entry };
  if (entry && typeof entry === 'object') {
    const record = entry as Record<string, unknown>;
    if (typeof record.id !== 'string') return null;
    return {
      id: record.id,
      ...(typeof record.name === 'string' ? { name: record.name } : {}),
      ...(typeof record.present_character_count === 'number'
        ? { present_character_count: record.present_character_count }
        : {}),
      ...(typeof record.active_client_count === 'number'
        ? { active_client_count: record.active_client_count }
        : {}),
      ...(typeof record.is_loaded === 'boolean' ? { is_loaded: record.is_loaded } : {}),
      ...(Array.isArray(record.present_characters)
        ? {
            present_characters: record.present_characters
              .map((id) => Number(id))
              .filter((id) => Number.isFinite(id)),
          }
        : {}),
    };
  }
  return null;
}

/**
 * Commands and requests initiated by a client WebSocket connection.
 * Conforms to `crates/alhalte-banquette/src/protocol.rs`.
 */
export type ClientFrame =
  | 'ListScenes'
  | { ListScenes?: Record<string, never> }
  | { QueryScene: { scene_id: string } }
  | { CreateScene: { scene_id: string; inherit_juices: boolean; name?: string | null } }
  | { DeleteScene: { scene_id: string } }
  | { EnterScene: { scene_id: string } }
  | { LeaveScene: { scene_id: string } }
  | { SetDone: { done: boolean } }
  | { QueryState: { scene_id: string } }
  | { UpdateState: { scene_id: string; key: string; value: TinJson | any } }
  | { DeleteState: { scene_id: string; key: string } }
  | { AppendLog: { scene_id: string; entry: TinJson | any | null } }
  | { EditLog: { scene_id: string; chunk_index: number; position: number; entry: TinJson | any } }
  | { DeleteLog: { scene_id: string; chunk_index: number; position: number } }
  | { RewindLog: { scene_id: string; chunk_index: number; position: number } }
  | { QueryLogBiblio: { scene_id: string } }
  | { QueryLogChunk: { scene_id: string; chunk_index: number } }
  | 'ListCharacters'
  | { ListCharacters?: Record<string, never> }
  | { QueryCharacter: { character_id: number } }
  | { CreateCharacter: { data: any; character_id?: number } }
  | { UpdateCharacter: { character_id: number; data: any } }
  | { DeleteCharacter: { character_id: number } }
  | { MoveCharacter: { character_id: number; to_scene_id: string } }
  | { ChangeBgm: { scene_id: string; slot: number; tin_id: string | null } }
  | { WebRtcSignal: { to_client_id: number; payload: any } }
  | 'HeartbeatResponse';

/**
 * Messages emitted from banquette to a client WebSocket.
 * Conforms to `crates/alhalte-banquette/src/protocol.rs`.
 */
export type ServerFrame =
  | { SceneList: { scenes: SceneListEntry[] } }
  | { SceneInfo: SceneSummary }
  | { StateSnapshot: { scene_id: string; states: Record<string, any> } }
  | { StateUpdated: { scene_id: string; key: string; value: any } }
  | { StateDeleted: { scene_id: string; key: string } }
  | { LogBiblioSnapshot: { scene_id: string; biblio: any } }
  | { LogChunkSnapshot: { scene_id: string; chunk_index: number; entries: any[] } }
  | { LogEventAppended: { scene_id: string; chunk_index: number; position: number; entry: any } }
  | { LogEventEdited: { scene_id: string; chunk_index: number; position: number; entry: any } }
  | { LogEventDeleted: { scene_id: string; chunk_index: number; position: number } }
  | { LogEventRewound: { scene_id: string; chunk_index: number; position: number } }
  | { CharacterList: { characters: Record<string | number, any> } }
  | { CharacterInfo: { character_id: number; data: any } }
  | { CharacterCreated: { character_id: number } }
  | { CharacterUpdated: { character_id: number } }
  | { CharacterDeleted: { character_id: number } }
  | { WebRtcSignal: { from_client_id: number; payload: any } }
  | 'HeartbeatPing'
  | { HeartbeatPing: Record<string, never> }
  | { SceneCreated: { scene_id: string } }
  | { SceneDeleted: { scene_id: string } }
  | { CharacterMoved: { character_id: number; to_scene_id: string } }
  | { DoneUpdated: { done: boolean } }
  | { Error: { code: string; message: string } };

/**
 * Configuration options for RoomSocket.
 */
export interface RoomSocketOptions {
  /** Room identifier */
  roomId: string | number;
  /** Explicit WebSocket URL (e.g., ws://host/ws/room/123) */
  url?: string;
  /** Server host override (e.g., localhost:3000) */
  host?: string;
  /** Authenticated account ID for query string */
  accountId?: number | string;
  /** Authenticated session ID for query string */
  sessionId?: number | string;
  /** ConnectionManager instance (defaults to shared singleton) */
  connectionManager?: ConnectionManager;
  /** ChunkLogManager instance for log sync */
  chunkLogManager?: ChunkLogManager;
  /** Auto connect upon initialization (defaults to false) */
  autoConnect?: boolean;
  /** Initial scene ID to enter (defaults to '/') */
  initialSceneId?: string;
  /** Reconnection delay in milliseconds (defaults to 2000) */
  reconnectInterval?: number;
  /** Maximum reconnect attempts (defaults to Infinity) */
  maxReconnectAttempts?: number;
  /** Custom WebSocket factory for testing */
  webSocketFactory?: (url: string) => WebSocket;
  /**
   * Called for every `close` event, including intentional disconnects.
   *
   * The event's `code` / `reason` are also available through
   * {@link RoomSocket.getLastCloseEvent}.
   */
  onClose?: (event: CloseEvent) => void;

  // Frame event callbacks
  onSceneList?: (scenes: string[]) => void;
  onSceneInfo?: (scene: any) => void;
  onSceneCreated?: (sceneId: string) => void;
  onSceneDeleted?: (sceneId: string) => void;
  onCharacterList?: (characters: Character[]) => void;
  onCharacterInfo?: (characterId: number, data: any) => void;
  onCharacterCreated?: (characterId: number) => void;
  onCharacterUpdated?: (characterId: number, data?: any) => void;
  onCharacterDeleted?: (characterId: number) => void;
  onCharacterMoved?: (characterId: number, toSceneId: string) => void;
  onStateUpdated?: (sceneId: string, key: string, value: any) => void;
  onStateDeleted?: (sceneId: string, key: string) => void;
  onStateSnapshot?: (sceneId: string, states: Record<string, any>) => void;
  onLogBiblioSnapshot?: (sceneId: string, biblio: any) => void;
  onLogAppended?: (record: LogRecord) => void;
  onLogEventEdited?: (sceneId: string, chunkIndex: number, position: number, entry: any) => void;
  onLogEventDeleted?: (sceneId: string, chunkIndex: number, position: number) => void;
  onLogEventRewound?: (sceneId: string, chunkIndex: number, position: number) => void;
  onWebRtcSignal?: (fromClientId: number, payload: any) => void;
  onServerFrame?: (frame: ServerFrame) => void;
  onError?: (error: { code: string; message: string }) => void;
}

/**
 * Normalizes any value into a Tin container if not already one.
 */
export function ensureTin(val: any): any {
  if (val && typeof val === 'object' && 'label' in val && 'payload' in val) {
    return val;
  }
  if (typeof val === 'string') {
    return {
      label: Number(LABEL_PRIMITIVE_STRING),
      payload: Array.from(new TextEncoder().encode(val)),
    };
  }
  if (typeof val === 'number') {
    const buf = new Uint8Array(8);
    const view = new DataView(buf.buffer);
    view.setBigUint64(0, BigInt(Math.floor(val)), true);
    return {
      label: Number(LABEL_PRIMITIVE_U64),
      payload: Array.from(buf),
    };
  }
  if (typeof val === 'boolean') {
    return {
      label: Number(LABEL_PRIMITIVE_BOOL),
      payload: [val ? 1 : 0],
    };
  }
  return val;
}

/** Close codes that mean "do not retry this connection". */
export const NON_RETRYABLE_CLOSE_CODES: ReadonlySet<number> = new Set([1008, 4403, 4404]);

/** Returns the WebSocket origin for a page or socket URL protocol. */
function webSocketOrigin(protocol: string, host: string): string {
  return `${protocol === 'https:' || protocol === 'wss:' ? 'wss:' : 'ws:'}//${host}`;
}

/** Returns the default WebSocket origin from `window` or the `host` option. */
function defaultWebSocketOrigin(host?: string): string {
  const protocol =
    typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const targetHost = host || (typeof window !== 'undefined' ? window.location.host : 'localhost:3000');
  return `${protocol}//${targetHost}`;
}

/**
 * Builds the standard WebSocket URL for a room connection with optional
 * credentials.
 *
 * Invitation URLs (`https://host/rooms/<id>#<key>`) are page URLs, not socket
 * URLs: only their origin is used and the socket path is always
 * `/ws/room/<roomId>`. An explicit `ws(s)://.../ws/room/...` URL is used as-is
 * (plus credentials); any other URL is reduced to its origin.
 *
 * Reference: `web/alhalte-argenterie/ARCHITECTURE.md` (WebSocket).
 */
export function buildRoomWebSocketUrl(options: {
  roomId: string | number;
  url?: string;
  host?: string;
  accountId?: number | string;
  sessionId?: number | string;
}): string {
  const { roomId, url, host, accountId, sessionId } = options;

  let base: string;
  if (url) {
    const hashIdx = url.indexOf('#');
    const cleanUrl = hashIdx >= 0 ? url.slice(0, hashIdx) : url;

    try {
      if (/^wss?:\/\//i.test(cleanUrl)) {
        const parsed = new URL(cleanUrl);
        // Only an explicit room socket URL may be used as-is.
        base = /^\/ws\/room\/[^/]+\/?$/.test(parsed.pathname)
          ? `${webSocketOrigin(parsed.protocol, parsed.host)}${parsed.pathname}${parsed.search}`
          : `${webSocketOrigin(parsed.protocol, parsed.host)}/ws/room/${roomId}`;
      } else if (/^https?:\/\//i.test(cleanUrl)) {
        // Invitation / application page URL: keep only the origin.
        const parsed = new URL(cleanUrl);
        base = `${webSocketOrigin(parsed.protocol, parsed.host)}/ws/room/${roomId}`;
      } else {
        // Relative page path (`/rooms/<id>#<key>`, `/room/<id>`, ...).
        base = `${defaultWebSocketOrigin(host)}/ws/room/${roomId}`;
      }
    } catch {
      base = `${defaultWebSocketOrigin(host)}/ws/room/${roomId}`;
    }
  } else {
    base = `${defaultWebSocketOrigin(host)}/ws/room/${roomId}`;
  }

  const hashIdx = base.indexOf('#');
  if (hashIdx >= 0) {
    base = base.slice(0, hashIdx);
  }

  const queryIndex = base.indexOf('?');
  const pathname = queryIndex >= 0 ? base.slice(0, queryIndex) : base;
  const searchStr = queryIndex >= 0 ? base.slice(queryIndex + 1) : '';
  const params = new URLSearchParams(searchStr);

  if (accountId !== undefined && accountId !== null && !params.has('account_id')) {
    params.set('account_id', String(accountId));
  }
  if (sessionId !== undefined && sessionId !== null && !params.has('session_id')) {
    params.set('session_id', String(sessionId));
  }

  const queryString = params.toString();
  return queryString ? `${pathname}?${queryString}` : pathname;
}

/**
 * Converts a raw server log entry (Tin container, string, or object) into a LogRecord.
 */
export function entryToLogRecord(
  sceneId: string,
  entry: any,
  chunkIndex = 0,
  position = 0,
): LogRecord | null {
  if (entry === null || entry === undefined) return null;

  // Banquette system events are postcard payloads; decode by label first so
  // their bytes are never interpreted as UTF-8 chat content (Issue #161).
  const systemRecord = systemEventToLogRecord(entry, sceneId, chunkIndex, position);
  if (systemRecord) return systemRecord;

  // Direct LogRecord object
  if (
    typeof entry === 'object' &&
    typeof entry.content === 'string' &&
    typeof entry.speaker === 'string'
  ) {
    return parseLogRecord(
      { ...entry, sceneId: entry.sceneId || sceneId, chunkIndex, position },
      `log-${chunkIndex}-${position}`,
    );
  }

  // Tin container with payload
  if (typeof entry === 'object' && entry.payload) {
    try {
      const payloadBytes = Array.isArray(entry.payload)
        ? new Uint8Array(entry.payload)
        : entry.payload instanceof Uint8Array
          ? entry.payload
          : new Uint8Array(Object.values(entry.payload));
      const text = new TextDecoder().decode(payloadBytes);
      try {
        const json = JSON.parse(text);
        if (typeof json === 'object' && json !== null) {
          return parseLogRecord(
            { ...json, sceneId: json.sceneId || sceneId, chunkIndex, position },
            `log-${chunkIndex}-${position}`,
          );
        }
      } catch {
        // Plain text string payload
      }
      return parseLogRecord(
        { content: text, sceneId, chunkIndex, position },
        `log-${chunkIndex}-${position}`,
      );
    } catch {
      // Decode failed
    }
  }

  // Plain string
  if (typeof entry === 'string') {
    return parseLogRecord(
      { content: entry, sceneId, chunkIndex, position },
      `log-${chunkIndex}-${position}`,
    );
  }

  // Generic object
  if (typeof entry === 'object') {
    return parseLogRecord(
      { ...entry, sceneId: entry.sceneId || sceneId, chunkIndex, position },
      `log-${chunkIndex}-${position}`,
    );
  }

  return null;
}

/**
 * Kinds of correlated operations tracked while waiting for an acknowledgement.
 */
type OperationKind =
  | 'appendLog'
  | 'updateState'
  | 'deleteState'
  | 'createScene'
  | 'deleteScene'
  | 'moveCharacter'
  | 'createCharacter'
  | 'updateCharacter'
  | 'deleteCharacter'
  | 'rewindLog'
  | 'changeBgm'
  | 'setDone'
  | 'querySceneList'
  | 'queryState'
  | 'queryLogBiblio'
  | 'queryLogChunk'
  | 'queryListCharacters';

/** Result of testing a pending operation against an incoming frame. */
interface PendingMatch<T> {
  matched: boolean;
  value?: T;
}

/**
 * A Promise-based operation waiting for its acknowledgement frame.
 */
interface PendingOperation<T = any> {
  id: string;
  kind: OperationKind;
  errorCodes: string[];
  test: (frame: ServerFrame) => PendingMatch<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
  timer: ReturnType<typeof setTimeout> | null;
}

/**
 * Client managing WebSocket connection, reconnection, and ClientFrame/ServerFrame
 * synchronization for an active room.
 */
export class RoomSocket {
  private wsUrl: string;
  private options: RoomSocketOptions;
  private connectionManager: ConnectionManager;
  private chunkLogManager?: ChunkLogManager;
  private socket: WebSocket | null = null;
  private messageQueue: ClientFrame[] = [];
  private pendingStateChanges: Map<string, ClientFrame> = new Map();
  private otherOutgoingQueue: ClientFrame[] = [];
  private intentionalClose = false;
  private isReconnecting = false;
  private reconnectAttempts = 0;
  private reconnectTimer: any = null;
  private currentSceneId: string;
  private connectPromises: Array<{ resolve: () => void; reject: (err: any) => void }> = [];
  private pendingOperations: PendingOperation[] = [];
  private operationCounter = 0;
  private lastCloseEvent: CloseEvent | null = null;

  // Local state cache
  private scenes: string[] = [];
  /** Server-authoritative scene summaries keyed by scene id. */
  private sceneSummaries: Map<string, SceneSummary> = new Map();
  private characters: Character[] = [];
  private stateMap: Map<string, Map<string, any>> = new Map();

  // Observers
  private frameListeners: Set<(frame: ServerFrame) => void> = new Set();

  constructor(options: RoomSocketOptions) {
    this.options = options;
    this.connectionManager = options.connectionManager ?? defaultConnectionManager;
    this.chunkLogManager = options.chunkLogManager;
    this.currentSceneId = options.initialSceneId || '/';
    this.wsUrl = buildRoomWebSocketUrl(options);

    // Register chained reconnect handler with connectionManager
    const prevHandler = (this.connectionManager as any)._reconnectHandler;
    this.connectionManager.setReconnectHandler(async () => {
      if (prevHandler) {
        try {
          await prevHandler();
        } catch (err) {
          console.warn('Error in previous reconnect handler:', err);
        }
      }
      await this.reconnect();
    });

    if (options.autoConnect) {
      this.connect();
    }
  }

  /**
   * Sets or updates the active ChunkLogManager.
   */
  public setChunkLogManager(manager?: ChunkLogManager): void {
    this.chunkLogManager = manager;
  }

  /**
   * Current WebSocket target URL.
   */
  public getUrl(): string {
    return this.wsUrl;
  }

  /**
   * Most recent close event, including its `code` and `reason`.
   *
   * Returns `null` until the first close. Non-retryable codes
   * ({@link NON_RETRYABLE_CLOSE_CODES}) stop automatic reconnection.
   */
  public getLastCloseEvent(): CloseEvent | null {
    return this.lastCloseEvent;
  }

  /**
   * True if WebSocket connection is open.
   */
  public get isConnected(): boolean {
    return this.socket !== null && this.socket.readyState === 1; // WebSocket.OPEN
  }

  /**
   * True if currently connecting or reconnecting.
   */
  public get isConnecting(): boolean {
    return (
      this.socket !== null &&
      (this.socket.readyState === 0 || this.isReconnecting) // WebSocket.CONNECTING
    );
  }

  /**
   * Connects to the room WebSocket server.
   */
  public connect(): void {
    if (this.socket && (this.socket.readyState === 0 || this.socket.readyState === 1)) {
      return;
    }

    if (
      !this.options.webSocketFactory &&
      (this.wsUrl.includes('.example') || this.wsUrl.includes('://example.'))
    ) {
      // Reserved RFC 2606 .example domain used in documentation and unit test fixtures
      return;
    }

    this.intentionalClose = false;
    if (this.isReconnecting) {
      this.connectionManager.setState('reconnecting');
    } else {
      this.connectionManager.setState('connecting');
    }

    try {
      const ws = this.options.webSocketFactory
        ? this.options.webSocketFactory(this.wsUrl)
        : new WebSocket(this.wsUrl);
      this.socket = ws;

      ws.onopen = () => {
        this.connectionManager.setState('connected');
        this.isReconnecting = false;
        this.reconnectAttempts = 0;
        this.clearReconnectTimer();

        // Resolve any awaiting reconnect promises
        for (const { resolve } of this.connectPromises) {
          resolve();
        }
        this.connectPromises = [];

        // Flush queued frames
        this.flushQueue();

        // Enter initial scene & request room lists
        this.enterScene(this.currentSceneId);
        this.listScenes();
        this.listCharacters();
      };

      ws.onmessage = (event: MessageEvent) => {
        this.handleMessage(event.data);
      };

      ws.onerror = (event: Event) => {
        console.warn('RoomSocket error:', event);
      };

      ws.onclose = (event: CloseEvent) => {
        this.lastCloseEvent = event;
        this.options.onClose?.(event);

        const wasIntentional = this.intentionalClose;
        this.socket = null;

        if (wasIntentional) {
          this.connectionManager.setState('disconnected');
          for (const { reject } of this.connectPromises) {
            reject(new Error('WebSocket closed intentionally'));
          }
          this.connectPromises = [];
          this.rejectAllPendingOperations(
            new ServerOperationError('disconnected', 'WebSocket closed before the acknowledgement'),
          );
          return;
        }

        // Unexpected disconnect: update connectionManager and schedule auto-reconnection
        this.connectionManager.setState('disconnected');

        for (const { reject } of this.connectPromises) {
          reject(new Error('WebSocket connection closed'));
        }
        this.connectPromises = [];
        this.rejectAllPendingOperations(
          new ServerOperationError('disconnected', 'WebSocket connection lost before the acknowledgement'),
        );

        // 1008 (policy violation) and the application close codes 4403 / 4404
        // mean the server rejected the join; retrying cannot succeed.
        const retryable = !NON_RETRYABLE_CLOSE_CODES.has(event?.code);
        const maxAttempts = this.options.maxReconnectAttempts ?? Infinity;
        if (retryable && this.reconnectAttempts < maxAttempts) {
          this.reconnectAttempts++;
          this.clearReconnectTimer();
          const delay = this.options.reconnectInterval ?? 2000;
          this.reconnectTimer = setTimeout(() => {
            this.reconnect().catch(() => {});
          }, delay);
        }
      };
    } catch (err) {
      this.connectionManager.setState('disconnected');
      for (const { reject } of this.connectPromises) {
        reject(err);
      }
      this.connectPromises = [];
      this.rejectAllPendingOperations(
        new ServerOperationError('disconnected', 'WebSocket connection could not be established'),
      );
    }
  }

  /**
   * Attempts reconnection to the room WebSocket server.
   */
  public reconnect(): Promise<void> {
    this.clearReconnectTimer();
    this.intentionalClose = false;
    this.isReconnecting = true;
    this.connectionManager.setState('reconnecting');

    if (
      !this.options.webSocketFactory &&
      (this.wsUrl.includes('.example') || this.wsUrl.includes('://example.'))
    ) {
      this.isReconnecting = false;
      this.connectionManager.setState('connected');
      return Promise.resolve();
    }

    if (this.socket) {
      try {
        this.socket.close();
      } catch {
        // ignore
      }
      this.socket = null;
    }

    return new Promise<void>((resolve, reject) => {
      this.connectPromises.push({ resolve, reject });
      this.connect();
    });
  }

  /**
   * Disconnects and cleans up the WebSocket connection intentionally.
   */
  public disconnect(): void {
    this.intentionalClose = true;
    this.clearReconnectTimer();
    if (this.socket) {
      try {
        this.socket.close();
      } catch {
        // ignore
      }
      this.socket = null;
    }
    this.connectionManager.setState('disconnected');
  }

  /**
   * Alias for `disconnect()`.
   */
  public close(): void {
    this.disconnect();
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  /**
   * Flushes queued frames upon successful reconnection:
   * 1. Transmits deduplicated final state changes (UpdateState / DeleteState).
   * 2. Bulk-transmits queued chat messages in FIFO order (AppendLog).
   * 3. Transmits any other pending frames.
   */
  public flushQueue(): void {
    if (!this.isConnected || !this.socket) return;

    // 1. Final state changes
    for (const [, stateFrame] of this.pendingStateChanges) {
      this.socket.send(JSON.stringify(stateFrame));
    }
    this.pendingStateChanges.clear();

    // 2. Bulk send queued messages
    while (this.messageQueue.length > 0) {
      const msgFrame = this.messageQueue.shift()!;
      this.socket.send(JSON.stringify(msgFrame));
    }

    // 3. Other queued frames
    while (this.otherOutgoingQueue.length > 0) {
      const frame = this.otherOutgoingQueue.shift()!;
      this.socket.send(JSON.stringify(frame));
    }
  }

  /**
   * Returns all pending queued frames (state updates, messages, and other commands).
   */
  public get outgoingQueue(): ClientFrame[] {
    return [
      ...Array.from(this.pendingStateChanges.values()),
      ...this.messageQueue,
      ...this.otherOutgoingQueue,
    ];
  }

  /**
   * Returns a copy of the current offline message queue.
   */
  public getMessageQueue(): ClientFrame[] {
    return [...this.messageQueue];
  }

  /**
   * Returns count of queued offline messages.
   */
  public getPendingMessageCount(): number {
    return this.messageQueue.length;
  }

  /**
   * Returns a copy of the pending offline state changes (deduplicated final states).
   */
  public getPendingStateChanges(): Map<string, ClientFrame> {
    return new Map(this.pendingStateChanges);
  }

  /**
   * Clears all pending offline queues.
   */
  public clearPendingQueues(): void {
    this.messageQueue = [];
    this.pendingStateChanges.clear();
    this.otherOutgoingQueue = [];
  }

  // =========================================================================
  // ClientFrame Outgoing Methods
  // =========================================================================

  /**
   * Sends an arbitrary ClientFrame to the server (or queues it if offline).
   * - AppendLog frames are queued in messageQueue (preserving FIFO order for bulk send upon reconnect).
   * - UpdateState and DeleteState frames are keyed by scene_id and key in pendingStateChanges (deduplicating to sync only final state).
   * - Other frames are kept in otherOutgoingQueue.
   */
  public send(frame: ClientFrame): void {
    if (this.isConnected && this.socket) {
      this.socket.send(JSON.stringify(frame));
      return;
    }

    if (typeof frame === 'object' && frame !== null) {
      if ('AppendLog' in frame) {
        this.messageQueue.push(frame);
        return;
      }
      if ('UpdateState' in frame && frame.UpdateState) {
        const { scene_id, key } = frame.UpdateState;
        this.pendingStateChanges.set(`${scene_id}:::${key}`, frame);
        return;
      }
      if ('DeleteState' in frame && frame.DeleteState) {
        const { scene_id, key } = frame.DeleteState;
        this.pendingStateChanges.set(`${scene_id}:::${key}`, frame);
        return;
      }
      if ('EnterScene' in frame && frame.EnterScene) {
        this.currentSceneId = frame.EnterScene.scene_id;
        return;
      }
    }

    this.otherOutgoingQueue.push(frame);
  }

  /**
   * Enters a scene.
   */
  public enterScene(sceneId: string): void {
    this.currentSceneId = sceneId;
    this.send({ EnterScene: { scene_id: sceneId } });
  }

  /**
   * Leaves a scene.
   */
  public leaveScene(sceneId: string): void {
    this.send({ LeaveScene: { scene_id: sceneId } });
  }

  /**
   * Creates a new scene.
   */
  public createScene(sceneId: string, inheritJuices: boolean): void {
    this.send({ CreateScene: { scene_id: sceneId, inherit_juices: inheritJuices } });
  }

  /**
   * Deletes a scene (admin only).
   */
  public deleteScene(sceneId: string): void {
    this.send({ DeleteScene: { scene_id: sceneId } });
  }

  /**
   * Updates or inserts a key-value pair in scene state.
   */
  public updateState(sceneId: string, key: string, value: any): void {
    this.send({ UpdateState: { scene_id: sceneId, key, value: ensureTin(value) } });
  }

  /**
   * Deletes a key from scene state.
   */
  public deleteState(sceneId: string, key: string): void {
    this.send({ DeleteState: { scene_id: sceneId, key } });
  }

  /**
   * Appends a log entry to a scene (fire-and-forget).
   *
   * A plain record is encoded as a Tin with a JSON payload (including a
   * `clientOpId`) so that banquette can decode it. Use
   * {@link RoomSocket.requestAppendLog} when the result must be awaited.
   */
  public appendLog(sceneId: string, entry: any): void {
    const wireEntry =
      entry === null || entry === undefined
        ? null
        : isTinJson(entry)
          ? jsonToTin(entry)
          : logRecordToTin(entry, this.nextOperationId('appendLog'));
    this.send({ AppendLog: { scene_id: sceneId, entry: wireEntry } });
  }

  /**
   * Creates a character. Supports `createCharacter(characterId, data)` or `createCharacter(data)`.
   */
  public createCharacter(characterIdOrData: number | any, data?: any): void {
    if (data !== undefined) {
      this.send({
        CreateCharacter: {
          character_id: typeof characterIdOrData === 'number' ? characterIdOrData : undefined,
          data,
        },
      });
    } else {
      this.send({
        CreateCharacter: {
          data: characterIdOrData,
        },
      });
    }
  }

  /**
   * Updates an existing character.
   */
  public updateCharacter(characterId: number, data: any): void {
    this.send({ UpdateCharacter: { character_id: characterId, data } });
  }

  /**
   * Deletes a character.
   */
  public deleteCharacter(characterId: number): void {
    this.send({ DeleteCharacter: { character_id: characterId } });
  }

  /**
   * Moves a character to a scene.
   */
  public moveCharacter(characterId: number, toSceneId: string): void {
    this.send({ MoveCharacter: { character_id: characterId, to_scene_id: toSceneId } });
  }

  /**
   * Sets the room's done flag (admin only), fire-and-forget.
   *
   * Use {@link RoomSocket.requestSetDone} when the caller must only report
   * success after banquette acknowledged the change.
   */
  public setDone(done: boolean): void {
    this.send({ SetDone: { done } });
  }

  /**
   * Sets the room's done flag and resolves only when banquette acknowledges it
   * with `DoneUpdated`.
   *
   * banquette sends `SetDoneFailed` for a non-admin request and maitre-d never
   * persists `rooms.done` before the success frame, so a rejected call leaves
   * both the DB and the UI unchanged. `PermissionDenied` rejects the pending
   * operation as well.
   */
  public requestSetDone(done: boolean, timeoutMs?: number): Promise<DoneUpdatedAck> {
    const promise = this.registerOperation<DoneUpdatedAck>(
      'setDone',
      ['SetDoneFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('DoneUpdated' in frame)) {
          return { matched: false };
        }
        if (Boolean(frame.DoneUpdated.done) !== Boolean(done)) {
          return { matched: false };
        }
        return { matched: true, value: frame.DoneUpdated };
      },
      timeoutMs,
    );

    this.send({ SetDone: { done } });
    return promise;
  }

  /**
   * Requests the full list of scenes.
   */
  public listScenes(): void {
    this.send('ListScenes');
  }

  /**
   * Requests the list of characters.
   */
  public listCharacters(): void {
    this.send('ListCharacters');
  }

  // =========================================================================
  // Promise-based operations (success / error / timeout handling)
  // =========================================================================

  /**
   * Returns the number of operations still waiting for an acknowledgement.
   *
   * Exposed for diagnostics and tests: every operation is removed from the
   * pending set when it settles or times out.
   */
  public getPendingOperationCount(): number {
    return this.pendingOperations.length;
  }

  /**
   * Generates a unique operation correlation ID.
   */
  private nextOperationId(kind: OperationKind): string {
    this.operationCounter += 1;
    return `${kind}-${this.operationCounter}-${Date.now().toString(36)}`;
  }

  /**
   * Registers a pending operation and returns its Promise.
   *
   * The caller is responsible for sending the corresponding frame (usually via
   * {@link RoomSocket.send}) after registration. The operation is always removed
   * from the pending set when it resolves, rejects, or times out.
   */
  private registerOperation<T>(
    kind: OperationKind,
    errorCodes: string[],
    test: (frame: ServerFrame) => PendingMatch<T>,
    timeoutMs?: number,
    id?: string,
  ): Promise<T> {
    const effectiveTimeout =
      typeof timeoutMs === 'number' && timeoutMs > 0 ? timeoutMs : DEFAULT_OPERATION_TIMEOUT_MS;
    const operationId = id ?? this.nextOperationId(kind);

    return new Promise<T>((resolve, reject) => {
      const operation: PendingOperation<T> = {
        id: operationId,
        kind,
        errorCodes,
        test,
        resolve,
        reject,
        timer: null,
      };
      operation.timer = setTimeout(() => {
        this.removePendingOperation(operation);
        reject(new OperationTimeoutError(kind, effectiveTimeout));
      }, effectiveTimeout);
      this.pendingOperations.push(operation);
    });
  }

  /**
   * Removes an operation from the pending set and clears its timeout timer.
   */
  private removePendingOperation(operation: PendingOperation): void {
    if (operation.timer !== null) {
      clearTimeout(operation.timer);
      operation.timer = null;
    }
    const index = this.pendingOperations.indexOf(operation);
    if (index >= 0) {
      this.pendingOperations.splice(index, 1);
    }
  }

  /**
   * Rejects every pending operation, e.g. when the socket closes.
   */
  private rejectAllPendingOperations(error: unknown): void {
    for (const operation of [...this.pendingOperations]) {
      this.removePendingOperation(operation);
      operation.reject(error);
    }
  }

  /**
   * Matches an incoming ServerFrame against pending operations and settles the
   * first (oldest) match. `ServerFrame::Error` rejects the matching operation by
   * `*Failed` code; `PermissionDenied` rejects all of them because it is not
   * correlated with a specific request.
   */
  private dispatchPendingOperations(frame: ServerFrame): void {
    if (typeof frame !== 'object' || frame === null) return;

    try {
      if ('Error' in frame) {
        const { code, message } = frame.Error;
        if (code === 'PermissionDenied') {
          this.rejectAllPendingOperations(new ServerOperationError(code, message));
          return;
        }
        const operation = this.pendingOperations.find((op) => op.errorCodes.includes(code));
        if (operation) {
          this.removePendingOperation(operation);
          operation.reject(new ServerOperationError(code, message));
        }
        return;
      }

      for (const operation of [...this.pendingOperations]) {
        const result = operation.test(frame);
        if (result.matched) {
          this.removePendingOperation(operation);
          operation.resolve(result.value);
          return;
        }
      }
    } catch (err) {
      console.warn('Failed to correlate a ServerFrame with a pending operation:', err);
    }
  }

  /**
   * Appends a log entry and resolves when the server broadcasts the matching
   * `LogEventAppended` echo (correlated by the embedded `clientOpId`).
   */
  public requestAppendLog(
    sceneId: string,
    record: LogRecord | Record<string, any>,
    timeoutMs?: number,
  ): Promise<LogAppendedAck> {
    const clientOpId = this.nextOperationId('appendLog');
    const entry = logRecordToTin(record, clientOpId);
    const promise = this.registerOperation<LogAppendedAck>(
      'appendLog',
      ['AppendLogFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('LogEventAppended' in frame)) {
          return { matched: false };
        }
        const event = frame.LogEventAppended;
        if (event.scene_id !== sceneId) return { matched: false };
        // Only the echo carrying our own clientOpId settles this operation, so
        // broadcasts of other clients never resolve our request.
        if (readClientOpId(event.entry) !== clientOpId) return { matched: false };
        return { matched: true, value: event };
      },
      timeoutMs,
      clientOpId,
    );

    this.send({ AppendLog: { scene_id: sceneId, entry } });
    return promise;
  }

  /**
   * Updates scene state and resolves when the matching `StateUpdated` arrives
   * (scene id, key, and value must all match).
   *
   * banquette acknowledges the requester directly in addition to broadcasting to
   * the target scene, so a sender inside the scene may receive the same frame
   * twice. The operation settles on the first copy and the duplicate is applied
   * idempotently to the cache.
   */
  public requestUpdateState(
    sceneId: string,
    key: string,
    value: any,
    timeoutMs?: number,
  ): Promise<StateUpdatedAck> {
    const tin = jsonToTin(value);
    const promise = this.registerOperation<StateUpdatedAck>(
      'updateState',
      ['UpdateStateFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('StateUpdated' in frame)) {
          return { matched: false };
        }
        const event = frame.StateUpdated;
        if (event.scene_id !== sceneId || event.key !== key) return { matched: false };
        if (!tinEquals(tin, event.value)) return { matched: false };
        return {
          matched: true,
          value: { scene_id: event.scene_id, key: event.key, value: tinToJson(event.value) },
        };
      },
      timeoutMs,
    );

    this.send({ UpdateState: { scene_id: sceneId, key, value: tin } });
    return promise;
  }

  /**
   * Deletes a scene state key and resolves when the matching `StateDeleted`
   * arrives.
   *
   * As with {@link RoomSocket.requestUpdateState}, banquette sends a direct ack
   * and a scene broadcast, so the sender may see the frame twice; the duplicate
   * is idempotent.
   */
  public requestDeleteState(
    sceneId: string,
    key: string,
    timeoutMs?: number,
  ): Promise<StateDeletedAck> {
    const promise = this.registerOperation<StateDeletedAck>(
      'deleteState',
      ['DeleteStateFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('StateDeleted' in frame)) {
          return { matched: false };
        }
        const event = frame.StateDeleted;
        if (event.scene_id !== sceneId || event.key !== key) return { matched: false };
        return { matched: true, value: event };
      },
      timeoutMs,
    );

    this.send({ DeleteState: { scene_id: sceneId, key } });
    return promise;
  }

  /**
   * Creates a scene and resolves when the server acknowledges with `SceneCreated`.
   *
   * `name` is written to the scene state `std::name` atomically at creation, so
   * `SceneSummary` / `SceneList` already carry the display name on the ack.
   */
  public requestCreateScene(
    sceneId: string,
    inheritJuices = false,
    name?: string,
    timeoutMs?: number,
  ): Promise<SceneAck> {
    const promise = this.registerOperation<SceneAck>(
      'createScene',
      ['CreateSceneFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('SceneCreated' in frame)) {
          return { matched: false };
        }
        const event = frame.SceneCreated;
        if (event.scene_id !== sceneId) return { matched: false };
        return { matched: true, value: event };
      },
      timeoutMs,
    );

    this.send({
      CreateScene: {
        scene_id: sceneId,
        inherit_juices: inheritJuices,
        ...(name !== undefined && name.length > 0 ? { name } : {}),
      },
    });
    return promise;
  }

  /**
   * Deletes a scene and resolves when the server acknowledges with `SceneDeleted`.
   */
  public requestDeleteScene(sceneId: string, timeoutMs?: number): Promise<SceneAck> {
    const promise = this.registerOperation<SceneAck>(
      'deleteScene',
      ['DeleteSceneFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('SceneDeleted' in frame)) {
          return { matched: false };
        }
        const event = frame.SceneDeleted;
        if (event.scene_id !== sceneId) return { matched: false };
        return { matched: true, value: event };
      },
      timeoutMs,
    );

    this.send({ DeleteScene: { scene_id: sceneId } });
    return promise;
  }

  /**
   * Moves a character to another scene and resolves when the server acknowledges
   * with `CharacterMoved`.
   */
  public requestMoveCharacter(
    characterId: number,
    toSceneId: string,
    timeoutMs?: number,
  ): Promise<CharacterMovedAck> {
    const promise = this.registerOperation<CharacterMovedAck>(
      'moveCharacter',
      ['MoveCharacterFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('CharacterMoved' in frame)) {
          return { matched: false };
        }
        const event = frame.CharacterMoved;
        if (Number(event.character_id) !== Number(characterId)) return { matched: false };
        if (event.to_scene_id !== toSceneId) return { matched: false };
        return { matched: true, value: event };
      },
      timeoutMs,
    );

    this.send({ MoveCharacter: { character_id: characterId, to_scene_id: toSceneId } });
    return promise;
  }

  /**
   * Creates a character from a `Character` model and resolves when the server
   * acknowledges with `CharacterCreated` (which carries the allocated ID).
   */
  public requestCreateCharacter(
    character: Character | Record<string, any>,
    timeoutMs?: number,
  ): Promise<CharacterAck> {
    // Character CRUD acknowledgements carry no correlation ID, so pending
    // operations are matched FIFO by kind. The UI serializes character saves.
    const promise = this.registerOperation<CharacterAck>(
      'createCharacter',
      ['CreateCharacterFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('CharacterCreated' in frame)) {
          return { matched: false };
        }
        return { matched: true, value: frame.CharacterCreated };
      },
      timeoutMs,
    );

    this.send({ CreateCharacter: { data: characterToTinMap(character) } });
    return promise;
  }

  /**
   * Updates a character from a `Character` model and resolves when the server
   * acknowledges with `CharacterUpdated`.
   *
   * Acknowledgements carry no correlation ID, so concurrent updates are matched
   * FIFO; the UI is expected to serialize character saves.
   */
  public requestUpdateCharacter(
    characterId: number,
    character: Character | Record<string, any>,
    timeoutMs?: number,
  ): Promise<CharacterAck> {
    const promise = this.registerOperation<CharacterAck>(
      'updateCharacter',
      ['UpdateCharacterFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('CharacterUpdated' in frame)) {
          return { matched: false };
        }
        return { matched: true, value: frame.CharacterUpdated };
      },
      timeoutMs,
    );

    this.send({ UpdateCharacter: { character_id: characterId, data: characterToTinMap(character) } });
    return promise;
  }

  /**
   * Deletes a character and resolves when the server acknowledges with
   * `CharacterDeleted`.
   */
  public requestDeleteCharacter(characterId: number, timeoutMs?: number): Promise<CharacterAck> {
    const promise = this.registerOperation<CharacterAck>(
      'deleteCharacter',
      ['DeleteCharacterFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('CharacterDeleted' in frame)) {
          return { matched: false };
        }
        return { matched: true, value: frame.CharacterDeleted };
      },
      timeoutMs,
    );

    this.send({ DeleteCharacter: { character_id: characterId } });
    return promise;
  }

  /**
   * Rewinds a scene log head and resolves when the scene broadcasts the matching
   * `LogEventRewound`.
   */
  public requestRewindLog(
    sceneId: string,
    chunkIndex: number,
    position: number,
    timeoutMs?: number,
  ): Promise<LogRewoundAck> {
    const promise = this.registerOperation<LogRewoundAck>(
      'rewindLog',
      ['RewindLogFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('LogEventRewound' in frame)) {
          return { matched: false };
        }
        const event = frame.LogEventRewound;
        if (
          event.scene_id !== sceneId ||
          Number(event.chunk_index) !== Number(chunkIndex) ||
          Number(event.position) !== Number(position)
        ) {
          return { matched: false };
        }
        return { matched: true, value: event };
      },
      timeoutMs,
    );

    this.send({ RewindLog: { scene_id: sceneId, chunk_index: chunkIndex, position } });
    return promise;
  }

  /**
   * Changes the BGM of a scene slot (or stops it with `tinId = null`).
   *
   * The server appends a `ChangeBgmEvent` log entry and announces it with
   * `LogEventAppended`; the matching broadcast resolves this operation.
   */
  public requestChangeBgm(
    sceneId: string,
    slot: number,
    tinId: string | null,
    timeoutMs?: number,
  ): Promise<LogAppendedAck> {
    const expectedTinId = tinId ? tinId.trim().toLowerCase() : null;
    const promise = this.registerOperation<LogAppendedAck>(
      'changeBgm',
      ['ChangeBgmFailed'],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('LogEventAppended' in frame)) {
          return { matched: false };
        }
        const event = frame.LogEventAppended;
        if (event.scene_id !== sceneId) return { matched: false };
        const decoded = decodeChangeBgmEntry(event.entry);
        if (!decoded) return { matched: false };
        if (decoded.slot !== null && decoded.slot !== Number(slot)) return { matched: false };
        if (decoded.tinIdHex !== expectedTinId) return { matched: false };
        return { matched: true, value: event };
      },
      timeoutMs,
    );

    this.send({ ChangeBgm: { scene_id: sceneId, slot, tin_id: tinId ?? null } });
    return promise;
  }

  /**
   * Requests the room scene list and resolves with the scene ids.
   *
   * Use {@link RoomSocket.getSceneSummaries} (or the `SceneInfo` callback) for
   * the server-authoritative names.
   */
  public querySceneList(timeoutMs?: number): Promise<string[]> {
    const promise = this.registerOperation<string[]>(
      'querySceneList',
      [],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('SceneList' in frame)) {
          return { matched: false };
        }
        return {
          matched: true,
          value: (frame.SceneList.scenes ?? []).map((entry) => sceneListEntryId(entry)),
        };
      },
      timeoutMs,
    );

    this.send('ListScenes');
    return promise;
  }

  /**
   * Requests a scene state snapshot and resolves with the decoded state map.
   *
   * Tin values are decoded with {@link tinToJson}, so `std::bgm` resolves to a
   * string and object states (e.g. `view::transform`) resolve to plain objects.
   */
  public queryState(sceneId: string, timeoutMs?: number): Promise<Record<string, any>> {
    const promise = this.registerOperation<Record<string, any>>(
      'queryState',
      [],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('StateSnapshot' in frame)) {
          return { matched: false };
        }
        const event = frame.StateSnapshot;
        if (event.scene_id !== sceneId) return { matched: false };
        const decoded: Record<string, any> = {};
        for (const [key, value] of Object.entries(event.states ?? {})) {
          decoded[key] = tinToJson(value);
        }
        return { matched: true, value: decoded };
      },
      timeoutMs,
    );

    this.send({ QueryState: { scene_id: sceneId } });
    return promise;
  }

  /**
   * Requests a scene log biblio and resolves with the `LogBiblio` payload.
   */
  public queryLogBiblio(sceneId: string, timeoutMs?: number): Promise<any> {
    const promise = this.registerOperation<any>(
      'queryLogBiblio',
      [],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('LogBiblioSnapshot' in frame)) {
          return { matched: false };
        }
        const event = frame.LogBiblioSnapshot;
        if (event.scene_id !== sceneId) return { matched: false };
        return { matched: true, value: event.biblio };
      },
      timeoutMs,
    );

    this.send({ QueryLogBiblio: { scene_id: sceneId } });
    return promise;
  }

  /**
   * Requests a log chunk and resolves with its raw entries (Tin values or
   * tombstones). Decode entries with {@link tinToLogRecord} when needed.
   */
  public queryLogChunk(
    sceneId: string,
    chunkIndex: number,
    timeoutMs?: number,
  ): Promise<Array<any>> {
    const promise = this.registerOperation<Array<any>>(
      'queryLogChunk',
      [],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('LogChunkSnapshot' in frame)) {
          return { matched: false };
        }
        const event = frame.LogChunkSnapshot;
        if (event.scene_id !== sceneId || Number(event.chunk_index) !== Number(chunkIndex)) {
          return { matched: false };
        }
        return { matched: true, value: event.entries };
      },
      timeoutMs,
    );

    this.send({ QueryLogChunk: { scene_id: sceneId, chunk_index: chunkIndex } });
    return promise;
  }

  /**
   * Requests the character list and resolves with decoded `Character` models.
   */
  public queryListCharacters(timeoutMs?: number): Promise<Character[]> {
    const promise = this.registerOperation<Character[]>(
      'queryListCharacters',
      [],
      (frame) => {
        if (typeof frame !== 'object' || frame === null || !('CharacterList' in frame)) {
          return { matched: false };
        }
        const list: Character[] = [];
        const characters = frame.CharacterList?.characters ?? {};
        for (const [id, raw] of Object.entries(characters)) {
          list.push(tinMapToCharacter(id, raw as Record<string, any>));
        }
        return { matched: true, value: list };
      },
      timeoutMs,
    );

    this.send('ListCharacters');
    return promise;
  }

  // =========================================================================
  // ServerFrame Incoming Handling
  // =========================================================================

  /**
   * Handles incoming WebSocket message.
   */
  public handleMessage(data: any): void {
    let frame: ServerFrame;
    try {
      if (typeof data === 'string') {
        const trimmed = data.trim();
        if (trimmed === 'HeartbeatPing' || trimmed === '"HeartbeatPing"') {
          frame = 'HeartbeatPing';
        } else {
          frame = JSON.parse(trimmed);
        }
      } else {
        return;
      }
    } catch (err) {
      console.warn('Failed to parse ServerFrame JSON:', err);
      return;
    }

    this.handleServerFrame(frame);
  }

  /**
   * Processes a structured ServerFrame.
   */
  public handleServerFrame(frame: ServerFrame): void {
    // 1. HeartbeatPing
    if (frame === 'HeartbeatPing' || (typeof frame === 'object' && 'HeartbeatPing' in frame)) {
      this.send('HeartbeatResponse');
      this.notifyServerFrame(frame);
      return;
    }

    if (typeof frame !== 'object' || frame === null) {
      this.notifyServerFrame(frame);
      return;
    }

    // Settle any Promise-based operation waiting for this frame. This runs before
    // the cache/callback handling so callers observe the acknowledgement promptly.
    this.dispatchPendingOperations(frame);

    // 2. SceneList (summaries; older servers may still send bare ids)
    if ('SceneList' in frame) {
      const entries = Array.isArray(frame.SceneList.scenes) ? frame.SceneList.scenes : [];
      this.sceneSummaries.clear();
      const ids: string[] = [];
      for (const entry of entries) {
        const summary = sceneListEntrySummary(entry);
        if (!summary) continue;
        this.sceneSummaries.set(summary.id, summary);
        ids.push(summary.id);
      }
      this.scenes = ids;
      this.options.onSceneList?.(ids);
    }

    // 2b. SceneInfo
    if ('SceneInfo' in frame) {
      const summary = sceneListEntrySummary(frame.SceneInfo);
      if (summary) this.sceneSummaries.set(summary.id, summary);
      this.options.onSceneInfo?.(frame.SceneInfo);
    }

    // 2c. SceneCreated / SceneDeleted keep the local scene cache consistent.
    if ('SceneCreated' in frame) {
      const { scene_id } = frame.SceneCreated;
      if (!this.scenes.includes(scene_id)) {
        this.scenes = [...this.scenes, scene_id];
      }
      this.options.onSceneCreated?.(scene_id);
    }

    if ('SceneDeleted' in frame) {
      const { scene_id } = frame.SceneDeleted;
      this.scenes = this.scenes.filter((id) => id !== scene_id);
      this.sceneSummaries.delete(scene_id);
      this.stateMap.delete(scene_id);
      this.options.onSceneDeleted?.(scene_id);
    }

    // 3. StateSnapshot replaces the cached state of the scene.
    if ('StateSnapshot' in frame) {
      const { scene_id, states } = frame.StateSnapshot;
      const next = new Map<string, any>();
      const decoded: Record<string, any> = {};
      for (const [key, value] of Object.entries(states ?? {})) {
        const plain = tinToJson(value);
        next.set(key, plain);
        decoded[key] = plain;
      }
      this.stateMap.set(scene_id, next);
      this.options.onStateSnapshot?.(scene_id, decoded);
    }

    // 3b. StateUpdated (Tin values are decoded for callers)
    if ('StateUpdated' in frame) {
      const { scene_id, key } = frame.StateUpdated;
      const value = tinToJson(frame.StateUpdated.value);
      let scMap = this.stateMap.get(scene_id);
      if (!scMap) {
        scMap = new Map();
        this.stateMap.set(scene_id, scMap);
      }
      scMap.set(key, value);
      this.options.onStateUpdated?.(scene_id, key, value);
    }

    // 4. StateDeleted
    if ('StateDeleted' in frame) {
      const { scene_id, key } = frame.StateDeleted;
      const scMap = this.stateMap.get(scene_id);
      if (scMap) {
        scMap.delete(key);
      }
      this.options.onStateDeleted?.(scene_id, key);
    }

    // 4b. LogBiblioSnapshot
    if ('LogBiblioSnapshot' in frame) {
      const { scene_id, biblio } = frame.LogBiblioSnapshot;
      // The biblio is server-authoritative: cache-origin records for the scene
      // are cleared here and the chunk list is (re)registered for lazy loads.
      this.chunkLogManager?.applyServerBiblio(scene_id, biblio);
      this.options.onLogBiblioSnapshot?.(scene_id, biblio);
    }

    // 5. LogEventAppended
    if ('LogEventAppended' in frame) {
      const { scene_id, chunk_index, position, entry } = frame.LogEventAppended;
      const record = entryToLogRecord(scene_id, entry, chunk_index, position);
      if (record) {
        this.chunkLogManager?.appendRecord(record);
        this.options.onLogAppended?.(record);
      }
    }

    // 5b. LogEventEdited / LogEventDeleted / LogEventRewound
    if ('LogEventEdited' in frame) {
      const { scene_id, chunk_index, position, entry } = frame.LogEventEdited;
      this.options.onLogEventEdited?.(scene_id, chunk_index, position, entry);
    }

    if ('LogEventDeleted' in frame) {
      const { scene_id, chunk_index, position } = frame.LogEventDeleted;
      this.options.onLogEventDeleted?.(scene_id, chunk_index, position);
    }

    if ('LogEventRewound' in frame) {
      const { scene_id, chunk_index, position } = frame.LogEventRewound;
      this.options.onLogEventRewound?.(scene_id, chunk_index, position);
    }

    // 6. LogChunkSnapshot
    if ('LogChunkSnapshot' in frame) {
      const { scene_id, chunk_index, entries } = frame.LogChunkSnapshot;
      if (Array.isArray(entries)) {
        const records: LogRecord[] = [];
        entries.forEach((entry, pos) => {
          const record = entryToLogRecord(scene_id, entry, chunk_index, pos);
          if (record) records.push(record);
        });
        // A snapshot is authoritative for its chunk: replacing the chunk's
        // entries (instead of appending) means a cached / deleted record cannot
        // survive a server response.
        this.chunkLogManager?.ingestChunkSnapshot(scene_id, chunk_index, records);
        for (const record of records) {
          this.options.onLogAppended?.(record);
        }
      }
    }

    // 7. CharacterList
    if ('CharacterList' in frame) {
      const charList: Character[] = [];
      if (frame.CharacterList?.characters) {
        for (const [id, raw] of Object.entries(frame.CharacterList.characters)) {
          // Decode the full Tin attribute map: statuses, parameters, chat
          // palette, icon, color and piece placement must match the character
          // tab instead of falling back to defaults (Issue #160).
          charList.push(tinMapToCharacter(id, raw as Record<string, any>));
        }
      }
      this.characters = charList;
      this.options.onCharacterList?.(charList);
    }

    // 7b. CharacterInfo updates a single character in the local cache.
    if ('CharacterInfo' in frame) {
      const { character_id, data } = frame.CharacterInfo;
      const parsed = tinMapToCharacter(character_id, data);
      const index = this.characters.findIndex((c) => c.id === String(character_id));
      if (index >= 0) {
        const next = [...this.characters];
        next[index] = parsed;
        this.characters = next;
      } else {
        this.characters = [...this.characters, parsed];
      }
      this.options.onCharacterInfo?.(character_id, data);
    }

    // 8. CharacterCreated
    if ('CharacterCreated' in frame) {
      this.options.onCharacterCreated?.(frame.CharacterCreated.character_id);
    }

    // 9. CharacterUpdated
    if ('CharacterUpdated' in frame) {
      this.options.onCharacterUpdated?.(frame.CharacterUpdated.character_id);
    }

    // 10. CharacterDeleted
    if ('CharacterDeleted' in frame) {
      const idStr = String(frame.CharacterDeleted.character_id);
      this.characters = this.characters.filter((c) => c.id !== idStr);
      this.options.onCharacterDeleted?.(frame.CharacterDeleted.character_id);
    }

    // 10b. CharacterMoved
    if ('CharacterMoved' in frame) {
      const { character_id, to_scene_id } = frame.CharacterMoved;
      this.options.onCharacterMoved?.(character_id, to_scene_id);
    }

    // 10c. WebRtcSignal relays a peer's signaling payload.
    if ('WebRtcSignal' in frame) {
      const { from_client_id, payload } = frame.WebRtcSignal;
      this.options.onWebRtcSignal?.(from_client_id, payload);
    }

    // 11. Error
    if ('Error' in frame) {
      this.options.onError?.(frame.Error);
    }

    this.notifyServerFrame(frame);
  }

  // =========================================================================
  // Subscriptions & State Accessors
  // =========================================================================

  /**
   * Subscribes a listener to all incoming ServerFrames.
   */
  public subscribe(listener: (frame: ServerFrame) => void): () => void {
    this.frameListeners.add(listener);
    return () => {
      this.frameListeners.delete(listener);
    };
  }

  private notifyServerFrame(frame: ServerFrame): void {
    this.options.onServerFrame?.(frame);
    for (const listener of this.frameListeners) {
      try {
        listener(frame);
      } catch (e) {
        console.error('Error in ServerFrame listener:', e);
      }
    }
  }

  /**
   * Current cached scenes.
   */
  public getScenes(): string[] {
    return [...this.scenes];
  }

  /**
   * Current cached scene summaries (IDs and server-authoritative names).
   */
  public getSceneSummaries(): SceneSummary[] {
    return [...this.sceneSummaries.values()];
  }

  /**
   * Current cached characters.
   */
  public getCharacters(): Character[] {
    return [...this.characters];
  }

  /**
   * Current cached scene state map.
   */
  public getState(sceneId: string): Map<string, any> | undefined {
    return this.stateMap.get(sceneId);
  }
}

/**
 * Factory helper function to create a RoomSocket.
 */
export function createRoomSocket(options: RoomSocketOptions): RoomSocket {
  return new RoomSocket(options);
}
