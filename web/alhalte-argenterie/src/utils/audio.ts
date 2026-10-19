/**
 * BGM audio playback manager (Issue #164).
 *
 * There is exactly one `HTMLAudioElement` per BGM slot. The manager:
 *
 * - downloads and decrypts a selected track through `downloadTin` with the room
 *   key, then creates an object URL from the decrypted payload,
 * - plays / pauses / changes the volume of that element,
 * - revokes the object URL when a slot stops, the track changes, the scene
 *   switches or the component unmounts,
 * - surfaces autoplay rejection as `blocked` (the UI offers a click-to-enable
 *   affordance) and download / decrypt failure as `error`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM`
 * - GitHub Issue #164
 */
import { downloadTin } from './cellarTransfer.js';
import {
  BGM_AUDIO_MIME_FALLBACK,
  type BgmTrackState,
} from './sceneSettings.js';

/** Playback status of one BGM slot. */
export type AudioSlotStatus =
  | 'idle'
  | 'loading'
  | 'playing'
  | 'paused'
  | 'blocked'
  | 'error';

/** Observable snapshot of one slot. */
export interface AudioSlotState {
  slot: number;
  status: AudioSlotStatus;
  tinId: string | null;
  mimeType: string | null;
  volume: number;
  /** User-facing failure message; `null` when the slot is healthy. */
  error: string | null;
  /** True after `play()` was rejected by the autoplay policy. */
  blocked: boolean;
}

/** Result of {@link AudioManager.play}. */
export type AudioPlayResult = 'playing' | 'blocked';

/**
 * Minimal structural subset of `HTMLAudioElement` used by the manager.
 *
 * Tests inject a fake implementation so no real media decoding happens.
 */
export interface AudioElementLike {
  src: string;
  volume: number;
  paused?: boolean;
  play(): Promise<void> | void;
  pause(): void;
  removeAttribute?(name: string): void;
}

/** User-facing message when the room key cannot decrypt the audio. */
export const BGM_AUDIO_MISSING_KEY_ERROR = '部屋鍵がないため、BGMを再生できません';
/** User-facing message when the audio tin cannot be downloaded or decrypted. */
export const BGM_AUDIO_LOAD_ERROR = '音源を読み込めませんでした';

/** Options for {@link createAudioManager}. */
export interface AudioManagerOptions {
  /** Creates the single element kept for a slot (defaults to `new Audio()`). */
  createElement?: (slot: number) => AudioElementLike;
  /** Downloads and decrypts the payload (defaults to `downloadTin`). */
  download?: (tinId: string, roomKey: Uint8Array) => Promise<Uint8Array>;
  /** Creates the object URL (defaults to `URL.createObjectURL`). */
  createObjectUrl?: (blob: Blob) => string;
  /** Revokes the object URL (defaults to `URL.revokeObjectURL`). */
  revokeObjectUrl?: (url: string) => void;
  /** MIME type used when a track does not carry an audio type. */
  fallbackMimeType?: string;
}

interface SlotRuntime {
  element: AudioElementLike | null;
  tinId: string | null;
  mimeType: string | null;
  objectUrl: string | null;
  status: AudioSlotStatus;
  error: string | null;
  blocked: boolean;
}

interface PendingLoad {
  tinId: string;
  promise: Promise<void>;
}

/** True when a `play()` rejection means the browser blocked autoplay. */
export function isAutoplayBlockedError(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false;
  const name = (error as { name?: unknown }).name;
  if (name === 'NotAllowedError') return true;
  const message = (error as { message?: unknown }).message;
  return (
    typeof message === 'string' &&
    /autoplay|user gesture|not allowed/i.test(message)
  );
}

function createDefaultElement(): AudioElementLike {
  if (typeof Audio !== 'undefined') {
    return new Audio() as unknown as AudioElementLike;
  }
  if (typeof document !== 'undefined') {
    return document.createElement('audio') as unknown as AudioElementLike;
  }
  throw new Error('HTMLAudioElement is not available in this environment');
}

function defaultCreateObjectUrl(blob: Blob): string {
  if (typeof URL === 'undefined' || typeof URL.createObjectURL !== 'function') {
    throw new Error('URL.createObjectURL is not available in this environment');
  }
  return URL.createObjectURL(blob);
}

function defaultRevokeObjectUrl(url: string): void {
  if (typeof URL !== 'undefined' && typeof URL.revokeObjectURL === 'function') {
    URL.revokeObjectURL(url);
  }
}

/**
 * Slot-keyed BGM playback manager.
 *
 * One element is kept per slot for the lifetime of the manager, so repeated
 * snapshot hydration never creates duplicate elements or duplicate playback.
 */
export class AudioManager {
  private readonly createElement: (slot: number) => AudioElementLike;
  private readonly download: (tinId: string, roomKey: Uint8Array) => Promise<Uint8Array>;
  private readonly createObjectUrl: (blob: Blob) => string;
  private readonly revokeObjectUrl: (url: string) => void;
  private readonly fallbackMimeType: string;

  private readonly slots = new Map<number, SlotRuntime>();
  private readonly pending = new Map<number, PendingLoad>();
  private readonly volumes = new Map<number, number>();
  private readonly generations = new Map<number, number>();
  private readonly listeners = new Set<() => void>();

  constructor(options: AudioManagerOptions = {}) {
    this.createElement = options.createElement ?? (() => createDefaultElement());
    this.download =
      options.download ??
      (async (tinId, roomKey) => {
        const tin = await downloadTin(tinId, roomKey);
        return tin.payload;
      });
    this.createObjectUrl = options.createObjectUrl ?? defaultCreateObjectUrl;
    this.revokeObjectUrl = options.revokeObjectUrl ?? defaultRevokeObjectUrl;
    this.fallbackMimeType = options.fallbackMimeType ?? BGM_AUDIO_MIME_FALLBACK;
  }

  /** Subscribes to any slot state change; returns an unsubscribe function. */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Snapshot of every slot known to the manager, keyed by slot number. */
  getStates(): Record<number, AudioSlotState> {
    const states: Record<number, AudioSlotState> = {};
    for (const slot of this.slots.keys()) {
      states[slot] = this.getState(slot);
    }
    return states;
  }

  /** Snapshot of one slot. */
  getState(slot: number): AudioSlotState {
    const runtime = this.slots.get(slot);
    return {
      slot,
      status: runtime?.status ?? 'idle',
      tinId: runtime?.tinId ?? null,
      mimeType: runtime?.mimeType ?? null,
      volume: this.volumes.get(slot) ?? 1,
      error: runtime?.error ?? null,
      blocked: runtime?.blocked ?? false,
    };
  }

  /** Sets the slot volume, applying it to the element when one exists. */
  setVolume(slot: number, volume: number): void {
    const clamped = Number.isFinite(volume) ? Math.max(0, Math.min(1, volume)) : 1;
    this.volumes.set(slot, clamped);
    const runtime = this.slots.get(slot);
    if (runtime?.element) runtime.element.volume = clamped;
  }

  /**
   * Downloads, decrypts and prepares a slot for playback.
   *
   * The current playback (if any) keeps running until the new payload is ready;
   * a failed download leaves the previous track untouched in the element and
   * records the error.
   *
   * @throws when the room key is missing, the download fails, or the payload
   * cannot be turned into an object URL.
   */
  async prepare(
    slot: number,
    track: BgmTrackState,
    roomKey: Uint8Array | null | undefined,
  ): Promise<void> {
    const tinId = typeof track.tinId === 'string' ? track.tinId.trim().toLowerCase() : '';
    if (!tinId) {
      throw new Error('a BGM track requires a tin id');
    }

    const runtime = this.runtime(slot);
    if (runtime.tinId === tinId && runtime.objectUrl && runtime.status !== 'error') {
      return;
    }

    const inFlight = this.pending.get(slot);
    if (inFlight && inFlight.tinId === tinId) {
      return inFlight.promise;
    }

    if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
      this.fail(slot, BGM_AUDIO_MISSING_KEY_ERROR);
      throw new Error(BGM_AUDIO_MISSING_KEY_ERROR);
    }

    const generation = (this.generations.get(slot) ?? 0) + 1;
    this.generations.set(slot, generation);

    const promise = (async () => {
      try {
        const payload = await this.download(tinId, roomKey);
        if (this.generations.get(slot) !== generation) return;
        const mimeType =
          typeof track.mimeType === 'string' && track.mimeType.startsWith('audio/')
            ? track.mimeType
            : this.fallbackMimeType;
        const bytes = payload instanceof Uint8Array ? payload : Uint8Array.from(payload);
        const url = this.createObjectUrl(new Blob([bytes.slice()], { type: mimeType }));
        if (this.generations.get(slot) !== generation) {
          this.revokeObjectUrl(url);
          return;
        }

        const target = this.runtime(slot);
        const element = target.element ?? this.createElement(slot);
        target.element = element;
        const previousUrl = target.objectUrl;
        // A track change must stop the previous playback before swapping src.
        if (previousUrl && previousUrl !== url) {
          try {
            element.pause();
          } catch {
            // Replacing src below stops playback even if pause() throws.
          }
        }
        element.src = url;
        element.volume = this.volumes.get(slot) ?? 1;
        target.tinId = tinId;
        target.mimeType = mimeType;
        target.objectUrl = url;
        target.status = 'paused';
        target.error = null;
        target.blocked = false;
        if (previousUrl && previousUrl !== url) this.revokeObjectUrl(previousUrl);
        this.notify();
      } catch (error) {
        if (this.generations.get(slot) !== generation) return;
        this.fail(slot, BGM_AUDIO_LOAD_ERROR);
        throw error;
      } finally {
        if (this.pending.get(slot)?.tinId === tinId) this.pending.delete(slot);
      }
    })();

    this.pending.set(slot, { tinId, promise });
    return promise;
  }

  /**
   * Plays the prepared slot.
   *
   * A browser autoplay rejection is not an error: the slot becomes `blocked`
   * and the UI offers a click-to-enable affordance. Any other failure records
   * `error` and rejects.
   */
  async play(slot: number): Promise<AudioPlayResult> {
    const inFlight = this.pending.get(slot);
    if (inFlight) await inFlight.promise;

    const runtime = this.slots.get(slot);
    if (!runtime?.objectUrl || !runtime.element) {
      this.fail(slot, BGM_AUDIO_LOAD_ERROR);
      throw new Error(BGM_AUDIO_LOAD_ERROR);
    }
    if (runtime.status === 'playing') return 'playing';

    try {
      await runtime.element.play();
      runtime.status = 'playing';
      runtime.blocked = false;
      runtime.error = null;
      this.notify();
      return 'playing';
    } catch (error) {
      if (isAutoplayBlockedError(error)) {
        runtime.status = 'blocked';
        runtime.blocked = true;
        runtime.error = null;
        this.notify();
        return 'blocked';
      }
      this.fail(slot, BGM_AUDIO_LOAD_ERROR);
      throw error;
    }
  }

  /** Retries playback after the user gesture that satisfies the autoplay policy. */
  async retry(slot: number): Promise<AudioPlayResult> {
    return this.play(slot);
  }

  /**
   * Pauses the slot and revokes its object URL.
   *
   * The audio element itself is kept so a later play reuses the same element
   * (one element per slot for the manager lifetime).
   */
  pause(slot: number): void {
    const runtime = this.slots.get(slot);
    // Cancel an in-flight download even when the slot has no element yet;
    // otherwise a scene switch / unmount could still create an object URL.
    const hadPending = this.pending.delete(slot);
    if (!runtime && !hadPending) return;
    this.generations.set(slot, (this.generations.get(slot) ?? 0) + 1);
    if (!runtime) return;

    if (runtime.element) {
      try {
        runtime.element.pause();
      } catch {
        // A pause failure must never break the state update.
      }
    }
    if (runtime.objectUrl) {
      this.revokeObjectUrl(runtime.objectUrl);
      runtime.objectUrl = null;
    }
    runtime.tinId = null;
    runtime.mimeType = null;
    runtime.status = 'idle';
    runtime.error = null;
    runtime.blocked = false;
    this.notify();
  }

  /** Pauses every slot and revokes every object URL (scene switch / unmount). */
  stopAll(): void {
    for (const slot of new Set([...this.slots.keys(), ...this.pending.keys()])) {
      this.pause(slot);
    }
  }

  /** Records a failure without an element (missing key, download failure). */
  private fail(slot: number, message: string): void {
    const runtime = this.runtime(slot);
    runtime.status = 'error';
    runtime.error = message;
    runtime.blocked = false;
    this.notify();
  }

  private runtime(slot: number): SlotRuntime {
    let runtime = this.slots.get(slot);
    if (!runtime) {
      runtime = {
        element: null,
        tinId: null,
        mimeType: null,
        objectUrl: null,
        status: 'idle',
        error: null,
        blocked: false,
      };
      this.slots.set(slot, runtime);
    }
    return runtime;
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}

/** Creates the per-App audio manager. */
export function createAudioManager(options: AudioManagerOptions = {}): AudioManager {
  return new AudioManager(options);
}
