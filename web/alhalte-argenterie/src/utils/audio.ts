/**
 * BGM audio playback manager (Issue #164, Issue #248, Issue #250).
 *
 * There is exactly one `HTMLAudioElement` per BGM slot. The manager:
 *
 * - downloads and decrypts a selected track through `downloadTin` with the room
 *   key, then creates an object URL from the decrypted payload,
 * - plays / pauses / changes the volume of that element, looping the track,
 * - seeks to the shared playback position of the scene state on play
 *   (Issue #250, see {@link computeBgmOffsetMs}),
 * - revokes the object URL when a slot stops, the track changes, the scene
 *   switches or the component unmounts,
 * - treats a browser autoplay rejection as a prepared-but-`paused` slot: it is
 *   not an error and carries no user-facing notice. `App` retries playback from
 *   the first user gesture (pointerdown), so the user never manages autoplay
 *   (Issue #248), and the retry keeps the derived playback position (#250),
 * - surfaces download / decrypt failure as `error`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM`
 * - GitHub Issue #164, GitHub Issue #248, GitHub Issue #250
 */
import { downloadTin } from './cellarTransfer.js';
import {
  BGM_AUDIO_MIME_FALLBACK,
  type BgmTrackState,
} from './sceneSettings.js';

/**
 * Playback status of one BGM slot.
 *
 * - `idle`: no source is prepared (initial state, stopped, or cleared).
 * - `loading`: reserved for an in-flight preparation; `prepare()` keeps `idle`
 *   until the payload is ready.
 * - `paused`: a source is prepared and nothing is audible. A browser autoplay
 *   rejection also lands here — prepared, silent, and *not* an error
 *   (Issue #248).
 * - `playing`: the element is playing.
 * - `error`: download / decrypt failed; {@link AudioSlotState.error} carries the
 *   user-facing message.
 */
export type AudioSlotStatus = 'idle' | 'loading' | 'playing' | 'paused' | 'error';

/** Observable snapshot of one slot. */
export interface AudioSlotState {
  slot: number;
  status: AudioSlotStatus;
  tinId: string | null;
  mimeType: string | null;
  volume: number;
  /** User-facing failure message; `null` when the slot is healthy. */
  error: string | null;
}

/**
 * Result of {@link AudioManager.play}.
 *
 * - `playing`: the element started.
 * - `paused`: the browser's autoplay policy refused the call. The slot stays
 *   prepared and silent; replay it from inside a user gesture (Issue #248).
 */
export type AudioPlayResult = 'playing' | 'paused';

/**
 * Minimal structural subset of `HTMLAudioElement` used by the manager.
 *
 * Tests inject a fake implementation so no real media decoding happens.
 * `loop` / `currentTime` / `duration` are optional: a fake element that omits
 * them still works, the manager just cannot seek or wrap by the track duration.
 */
export interface AudioElementLike {
  src: string;
  volume: number;
  paused?: boolean;
  /** Media duration in **seconds**; `NaN` / `Infinity` while unknown. */
  duration?: number;
  /** Playback position in **seconds** (`HTMLAudioElement` semantics). */
  currentTime?: number;
  /** Whether the element restarts at 0 when it reaches the end. */
  loop?: boolean;
  play(): Promise<void> | void;
  pause(): void;
  removeAttribute?(name: string): void;
}

/** Input of {@link computeBgmOffsetMs}; every timestamp is UNIX **milliseconds**. */
export interface BgmOffsetInput {
  /** Wall-clock "now" in UNIX milliseconds. */
  now: number;
  /** `bgm::<slot>::started_at`; absent for a legacy room / a never played slot. */
  startedAt?: number | null;
  /** `bgm::<slot>::paused_at`; absent while playing. */
  pausedAt?: number | null;
  /** Track duration in milliseconds; absent / `NaN` when the browser does not know it. */
  durationMs?: number | null;
}

/** True when `value` is a finite, non-negative number. */
function isFiniteNonNegative(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0;
}

/** Non-negative modulo; the result is `0 <= result < divisor`. */
function positiveModulo(value: number, divisor: number): number {
  return ((value % divisor) + divisor) % divisor;
}

/**
 * Derives the shared playback position of a BGM slot from the scene state
 * (Issue #250).
 *
 * The scene state stores only timestamps, never a duration, because the
 * duration belongs to the audio element of each client. This helper therefore
 * accepts the known duration and falls back to the raw elapsed time when the
 * browser has not reported one yet (the element then clamps / loops by itself).
 *
 * - playing: `((now - startedAt) mod D + D) mod D`
 * - paused: `((min(now, pausedAt) - startedAt) mod D + D) mod D`, so the
 *   position stays frozen at the pause instant even as `now` advances
 * - a missing / malformed `startedAt` (legacy room, pre-#250 state) is `0`
 * - a negative / `NaN` input is clamped to `0`
 *
 * Every client computes the same value from the same wall-clock state, which is
 * what lets a client joining mid-track start at the right offset. This assumes
 * the clients' clocks agree (NTP); see the PR body for the residual risk.
 */
export function computeBgmOffsetMs(input: BgmOffsetInput): number {
  const { startedAt, pausedAt, durationMs } = input;
  const now = isFiniteNonNegative(input.now) ? input.now : 0;
  if (!isFiniteNonNegative(startedAt)) return 0;

  const reference = isFiniteNonNegative(pausedAt) ? Math.min(now, pausedAt) : now;
  // A clock that jumped backwards (or a pause before the start) must not seek to
  // the end of the loop: the elapsed time is clamped at 0.
  const elapsed = Math.max(0, reference - startedAt);
  if (isFiniteNonNegative(durationMs) && durationMs > 0) {
    return positiveModulo(elapsed, durationMs);
  }
  // Unknown duration: the browser clamps / loops by itself, so hand it the raw
  // non-negative offset.
  return elapsed;
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
}

interface PendingLoad {
  tinId: string;
  promise: Promise<void>;
}

/**
 * True when a `play()` rejection is the browser's autoplay policy.
 *
 * `NotAllowedError` is the specified rejection; some engines only expose a
 * message, so both are inspected. Deliberately not exported: since Issue #248 an
 * autoplay rejection is an internal "prepared but paused" condition rather than
 * a user-facing state.
 */
function isAutoplayPolicyRejection(error: unknown): boolean {
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
   * Current playback position of a slot in milliseconds (Issue #250).
   *
   * `null` when no element exists or the element does not expose `currentTime`;
   * `App` only re-seeks a playing slot when this differs from the derived
   * position by more than a second, so an unrelated state update never stutters
   * the audio.
   */
  getPositionMs(slot: number): number | null {
    const element = this.slots.get(slot)?.element;
    if (!element || typeof element.currentTime !== 'number') return null;
    if (!Number.isFinite(element.currentTime)) return null;
    return element.currentTime * 1000;
  }

  /**
   * Track duration of a slot in milliseconds, or `null` while unknown.
   *
   * The scene state never stores a duration: this is the browser's own
   * `HTMLAudioElement.duration`, used to wrap the derived offset (Issue #250).
   */
  getDurationMs(slot: number): number | null {
    const element = this.slots.get(slot)?.element;
    if (!element || typeof element.duration !== 'number') return null;
    if (!Number.isFinite(element.duration) || element.duration <= 0) return null;
    return element.duration * 1000;
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
        // The track loops forever: the shared playback position is derived from
        // the wall clock and wrapped by the duration (Issue #250).
        element.loop = true;
        target.tinId = tinId;
        target.mimeType = mimeType;
        target.objectUrl = url;
        target.status = 'paused';
        target.error = null;
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
   * Plays the prepared slot, optionally seeking to a shared offset.
   *
   * `offsetMs` is the position derived from `bgm::<slot>::started_at` /
   * `::paused_at` (Issue #250): a client joining mid-track starts at the elapsed
   * position, and ⏪ seeks the already playing element back to 0. It is clamped
   * to `>= 0` and wrapped by the element's own duration when the browser reports
   * one; an unknown duration is passed through as the raw offset.
   *
   * A browser autoplay rejection is deliberately not an error (Issue #248): the
   * slot stays `paused` with `error === null` and this resolves to `'paused'`, so
   * the caller can replay it from inside a user gesture. Any other failure
   * records `error` and rejects.
   *
   * @throws when the slot has no prepared source or `play()` failed for a
   * reason other than the autoplay policy.
   */
  async play(slot: number, offsetMs?: number): Promise<AudioPlayResult> {
    const inFlight = this.pending.get(slot);
    if (inFlight) await inFlight.promise;

    const runtime = this.slots.get(slot);
    if (!runtime?.objectUrl || !runtime.element) {
      this.fail(slot, BGM_AUDIO_LOAD_ERROR);
      throw new Error(BGM_AUDIO_LOAD_ERROR);
    }
    // Seek before starting so a paused / freshly prepared element resumes at the
    // authoritative position instead of 0.
    this.seek(runtime.element, offsetMs);
    if (runtime.status === 'playing') return 'playing';

    try {
      await runtime.element.play();
      runtime.status = 'playing';
      runtime.error = null;
      this.notify();
      return 'playing';
    } catch (error) {
      if (isAutoplayPolicyRejection(error)) {
        runtime.status = 'paused';
        runtime.error = null;
        this.notify();
        return 'paused';
      }
      this.fail(slot, BGM_AUDIO_LOAD_ERROR);
      throw error;
    }
  }

  /**
   * Sets `element.currentTime` from a millisecond offset.
   *
   * A missing / malformed offset is a no-op, so an autoplay retry keeps the
   * position the element already has. The offset is wrapped by the element's own
   * duration when known (the scene state stores no duration, Issue #250).
   */
  private seek(element: AudioElementLike, offsetMs: number | undefined): void {
    if (typeof offsetMs !== 'number' || !Number.isFinite(offsetMs)) return;
    let targetMs = Math.max(0, offsetMs);
    if (
      typeof element.duration === 'number' &&
      Number.isFinite(element.duration) &&
      element.duration > 0
    ) {
      targetMs = positiveModulo(targetMs, element.duration * 1000);
    }
    try {
      element.currentTime = targetMs / 1000;
    } catch {
      // An element that cannot seek yet simply starts from its current position.
    }
  }

  /**
   * Pauses the slot and keeps its prepared source (Issue #250).
   *
   * The `tinId` / object URL stay in place, so resuming after a pause continues
   * from the paused position without downloading the tin again. Use
   * {@link stop} when the source is no longer needed (track change, deletion,
   * scene switch, unmount).
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
    // The source stays prepared: a paused slot is exactly what a resume needs.
    runtime.status = runtime.objectUrl ? 'paused' : 'idle';
    runtime.error = null;
    this.notify();
  }

  /**
   * Stops the slot and disposes its source (Issue #250).
   *
   * Pauses the element, revokes the object URL and forgets the tin, so the next
   * play downloads / decrypts again. Used for a track change, a deletion, a
   * scene switch and unmount.
   */
  stop(slot: number): void {
    const runtime = this.slots.get(slot);
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
    this.notify();
  }

  /** Stops every slot and revokes every object URL (scene switch / unmount). */
  stopAll(): void {
    for (const slot of new Set([...this.slots.keys(), ...this.pending.keys()])) {
      this.stop(slot);
    }
  }

  /** Records a failure without an element (missing key, download failure). */
  private fail(slot: number, message: string): void {
    const runtime = this.runtime(slot);
    runtime.status = 'error';
    runtime.error = message;
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
