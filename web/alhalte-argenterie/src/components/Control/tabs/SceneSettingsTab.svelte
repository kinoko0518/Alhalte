<script lang="ts">
  /**
   * Scene Settings Tab & Overlays.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成`
   * - GitHub Issue #228: メンバー移動 UI はキャラクター一覧へ移設
   * - GitHub Issue #32: Material 3 Expressive Switch, Slider, Carousel, 7:5:3 row & Material Color delete button
   * - GitHub Issue #109 / sub-issue #142: BGM状態変更のサーバー送信と送信中インジケーター
   * - GitHub Issue #226: BGMスロット削除 (track / playing / volume をサーバーから消す)
   * - GitHub Issue #250: BGM の再生位置同期 (started_at / paused_at) とトランスポート UI
   * - GitHub Issue #275: スロット追加フローの失敗を、行が無くても BGM 節に表示する
   */
  import {
    ButtonGroup,
    Button,
    Carousel,
    CarouselItem,
    CircularProgress,
    IconButton,
    Slider,
    Switch,
    TextField,
  } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import GenericImagePicker from '../GenericImagePicker.svelte';
  import type { BgmSlot, Scene } from '../../../types/index.js';
  import {
    BGM_AUDIO_MIME_FALLBACK,
    bgmPausedAtStateKey,
    bgmPlayingStateKey,
    bgmRemovedStateKey,
    bgmStartedAtStateKey,
    bgmTrackStateKey,
    bgmVolumeStateKey,
    buildBgmTrackState,
    clearSceneBackground,
    getBgmRemovedSlots,
    getNormalizedBgmSlots,
    SCENE_BACKGROUND_HEIGHT_STATE_KEY,
    SCENE_BACKGROUND_WIDTH_STATE_KEY,
    SCENE_NAME_STATE_KEY,
    setBgmSlotRemoved,
    uploadSceneBackground,
    type BgmTrackState,
  } from '../../../utils/sceneSettings.js';
  import {
    BGM_AUDIO_LOAD_ERROR,
    computeBgmOffsetMs,
    createAudioManager,
    type AudioManager,
    type AudioSlotState,
  } from '../../../utils/audio.js';
  import { ServerOperationError, type RoomSocket } from '../../../utils/roomSocket.js';
  import {
    loadAssets,
    subscribeAssets,
    type AssetItem,
    type AssetMediaType,
  } from '../../../utils/assets.js';
  import { roomRoleCapabilities, type RoomRole } from '../../../utils/session.js';
  import AssetPickerModal from '../AssetPickerModal.svelte';

  let {
    currentScene,
    role = 'participant',
    onUpdateScene,
    onCreateScene,
    onDeleteScene,
    onRequestDeleteScene,
    onRequestDeleteSceneLogs,
    onDeleteSceneLogs,
    onRequestAssetPicker,
    roomSocket = null,
    roomKey,
    audioManager = undefined,
    sceneActionPending = null,
    sceneActionError = null,
  }: {
    currentScene: Scene;
    /**
     * Role of the caller (Issue #201). Scene creation/deletion and log deletion
     * are admin-only; `viewer` and `denied` get the read-only tab.
     */
    role?: RoomRole;
    onUpdateScene?: (scene: Scene) => void;
    onCreateScene?: (
      name: string,
      inheritJuice: boolean,
    ) => Promise<boolean | void> | boolean | void;
    onDeleteScene?: (sceneId: string) => Promise<boolean | void> | boolean | void;
    onRequestDeleteScene?: (scene: { id: string; name: string }) => void;
    onRequestDeleteSceneLogs?: (scene: { id: string; name: string }) => void;
    onDeleteSceneLogs?: (sceneId: string) => Promise<boolean | void> | boolean | void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    /**
     * Room socket used to acknowledge BGM state changes (Issue #109 / #142),
     * scene rename / background / dimensions state (Issue #159).
     * When absent or disconnected the component keeps its local-only behaviour
     * only for standalone usage (no room key either).
     */
    roomSocket?: RoomSocket | null;
    /**
     * 32-byte room key used to encrypt the background image in the cellar
     * (Issue #159). `undefined` means "not resolved"; `null` means "no key".
     */
    roomKey?: Uint8Array | null;
    /**
     * Slot-keyed BGM audio manager (Issue #164). `App` owns the instance so a
     * scene switch / snapshot / remote play update and this tab share exactly
     * one `HTMLAudioElement` per slot. Standalone usage (tests / previews)
     * creates a private manager.
     */
    audioManager?: AudioManager;
    /**
     * Scene action whose control is replaced by a wavy `CircularProgress`
     * (`create` / `delete-logs` / `delete-scene` / `move:<characterId>`),
     * owned by `App` (Issue #109 / sub-issue #143).
     */
    sceneActionPending?: string | null;
    /** Failure text rendered under the management controls (Issue #109 / #143). */
    sceneActionError?: string | null;
  } = $props();

  /**
   * BGM audio manager (Issue #164). `App` passes its instance so playback is
   * shared with scene switches / state snapshots; standalone usage gets a
   * private manager so the component works without a room.
   */
  const bgmAudio = audioManager ?? createAudioManager();
  /** Per-slot playback state mirrored from the manager for rendering. */
  let bgmAudioStates = $state<Record<number, AudioSlotState>>({});

  $effect(() => {
    bgmAudioStates = bgmAudio.getStates();
    return bgmAudio.subscribe(() => {
      bgmAudioStates = bgmAudio.getStates();
    });
  });

  // Asset picker state
  let isAssetPickerOpen = $state(false);
  let assetPickerTarget = $state<'bgImage' | 'bgm1' | 'bgm2' | 'bgm3'>('bgImage');
  let assetPickerMediaType = $state<AssetMediaType>('image');
  let allAssets = $state<AssetItem[]>(loadAssets());

  $effect(() => {
    allAssets = loadAssets();
    const unsubscribe = subscribeAssets((updated) => {
      allAssets = updated;
    });
    return unsubscribe;
  });

  let capabilities = $derived(roomRoleCapabilities(role));

  /** Scene creation / deletion / log deletion / member moves are admin-only (Issue #201). */
  let canManageScenes = $derived(capabilities.canManageScenes);

  let imageAssets = $derived(
    allAssets.filter(
      (a) => a.mediaType === 'image' && (capabilities.isAdmin || a.scope === 'personal'),
    ),
  );

  // Overlays
  let isCreateSceneOpen = $state(false);
  let newSceneName = $state('');
  let inheritJuiceToggle = $state(true);

  let isDeleteConfirmOpen = $state(false);
  let isDeleteLogsConfirmOpen = $state(false);
  let hoveredAction = $state<'create' | 'delete-logs' | 'delete-scene' | null>(null);

  // Dimension inputs
  let inputWidth = $state<number | undefined>(undefined);
  let inputHeight = $state<number | undefined>(undefined);

  /** Background upload / dimension / clear error shown under the picker. */
  let bgImageError = $state<string | null>(null);
  /** True while the background image is being uploaded / published. */
  let bgImagePending = $state(false);
  /** Scene rename error shown under the scene name field. */
  let sceneNameError = $state<string | null>(null);
  /** Draft of the scene name; committed only after the server ack. */
  let sceneNameDraft = $state(currentScene.name);
  let sceneNameTimer: ReturnType<typeof setTimeout> | null = null;

  const SCENE_STATE_COMMUNICATION_ERROR = 'サーバーと通信できませんでした';
  const SCENE_BACKGROUND_MISSING_KEY_ERROR = '部屋鍵がないため、背景画像を保存できません';

  /**
   * True when the component runs standalone (tests / previews) rather than in a
   * room: no socket was supplied and no room key was even resolved.
   */
  function isStandalone(): boolean {
    return roomSocket === null && roomKey === undefined;
  }

  // Keep dimension inputs in sync with currentScene
  $effect(() => {
    inputWidth = currentScene.juices.bgWidth;
    inputHeight = currentScene.juices.bgHeight;
  });

  // Mirror external (snapshot / SceneInfo) scene name changes into the draft.
  // Only a real external change replaces the draft, so an unrelated re-render
  // does not discard in-progress typing.
  let lastSyncedSceneName = currentScene.name;
  $effect(() => {
    const externalName = currentScene.name;
    if (externalName !== lastSyncedSceneName) {
      lastSyncedSceneName = externalName;
      sceneNameDraft = externalName;
    }
  });

  // Drop a pending rename debounce when the component is destroyed.
  $effect(() => {
    return () => {
      if (sceneNameTimer) {
        clearTimeout(sceneNameTimer);
        sceneNameTimer = null;
      }
    };
  });

  /** Persists one background dimension, committing locally only after the ack. */
  async function commitDimension(axis: 'width' | 'height', value: number | undefined) {
    const key =
      axis === 'width' ? SCENE_BACKGROUND_WIDTH_STATE_KEY : SCENE_BACKGROUND_HEIGHT_STATE_KEY;
    const sceneId = currentScene.id;
    const socket = roomSocket;
    bgImageError = null;

    if (!socket || !socket.isConnected) {
      if (isStandalone()) {
        if (axis === 'width') currentScene.juices.bgWidth = value;
        else currentScene.juices.bgHeight = value;
        onUpdateScene?.(currentScene);
        return;
      }
      bgImageError = SCENE_STATE_COMMUNICATION_ERROR;
      return;
    }

    try {
      if (value === undefined) {
        await socket.requestDeleteState(sceneId, key);
      } else {
        await socket.requestUpdateState(sceneId, key, value);
      }
      if (currentScene.id !== sceneId) return;
      if (axis === 'width') currentScene.juices.bgWidth = value;
      else currentScene.juices.bgHeight = value;
      onUpdateScene?.(currentScene);
    } catch (error) {
      console.warn(`[SceneSettingsTab] Failed to persist the background ${axis}:`, error);
      bgImageError = SCENE_STATE_COMMUNICATION_ERROR;
      // Show the last committed value again; the state was not updated.
      inputWidth = currentScene.juices.bgWidth;
      inputHeight = currentScene.juices.bgHeight;
    }
  }

  function handleWidthChange(e: Event) {
    const raw = (e.target as HTMLInputElement).value.trim();
    const num = raw === '' ? NaN : Number(raw);
    const parsed = Number.isNaN(num) || num <= 0 ? undefined : num;
    inputWidth = parsed;
    void commitDimension('width', parsed);
  }

  function handleHeightChange(e: Event) {
    const raw = (e.target as HTMLInputElement).value.trim();
    const num = raw === '' ? NaN : Number(raw);
    const parsed = Number.isNaN(num) || num <= 0 ? undefined : num;
    inputHeight = parsed;
    void commitDimension('height', parsed);
  }


  function handleBgImageSelect() {
    assetPickerTarget = 'bgImage';
    assetPickerMediaType = 'image';
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'image',
        initialScope: 'personal',
        onSelect: (dataUrl) => {
          handleAssetPicked(dataUrl);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  // BGM Slots state & synchronization (Issue #53 / #109 / #142 / #164 / #250)
  //
  // State schema (see the PR 「判断した点」):
  // - track: `std::bgm` (slot 1), `std::bgm2` / `std::bgm3` (slots 2 / 3)
  //   as JSON Tin `{ tinId, mimeType, name }`; a legacy 64-hex string is still
  //   accepted. The track stays selected while stopped.
  // - playing: `bgm::<slot>::playing` boolean, written by play / stop and
  //   committed locally only after the ack.
  // - volume: `bgm::<slot>::volume` as a number in 0..1, sent on commit only.
  // - removed: `bgm::<slot>::removed` = true after the slot is deleted from the
  //   UI (Issue #226); it stays as a tombstone so a stale broadcast cannot
  //   resurrect the row. Slot numbers are never reused, so it is never cleared.
  // - started_at / paused_at: UNIX ms wall-clock timestamps of the current
  //   continuous playback (Issue #250). They are what every client derives its
  //   playback position from; no duration is ever stored.
  //   - play a new track: `started_at = now`, delete `paused_at`
  //   - pause: `paused_at = now`
  //   - resume: `started_at = now - (paused_at - started_at)`, delete `paused_at`
  //   - restart (⏪): `started_at = now`, delete `paused_at`
  //
  // A row exists iff the slot has a track (Issue #249): there is no empty slot
  // row, so `bgmSlots` only ever holds slots with a selected track.
  let bgmSlots = $state<BgmSlot[]>(getNormalizedBgmSlots(currentScene.juices));
  let activeBgmSlotPicking = $state<number | null>(null);
  /** Slots with an in-flight server operation; reassigned so Svelte sees the change. */
  let pendingBgmSlots = $state<Set<number>>(new Set());
  /**
   * Per-slot error message (Issue #275).
   *
   * A slot that has a row renders its message under the row
   * (`bgm-error-<slot>`); a slot without a row — every failure of the
   * "スロット追加" flow, because a row only exists after the track write
   * succeeded (Issue #249) — is rendered once under the section header
   * (`bgm-add-error`) by {@link bgmAddError}.
   */
  let bgmErrors = $state<Record<number, string>>({});
  /** Slider drafts: the committed volume lives in `bgmSlots` only after the ack. */
  let draftBgmVolumes = $state<Record<number, number>>({});

  const BGM_COMMUNICATION_ERROR = 'サーバーと通信できませんでした';
  const BGM_MISSING_TIN_ERROR = 'この音源はサーバーに保存されていません';
  const TIN_ID_HEX = /^[0-9a-f]{64}$/;

  $effect(() => {
    bgmSlots = getNormalizedBgmSlots(currentScene.juices);
  });

  // A scene switch invalidates in-flight BGM UI state (the ack still checks the
  // scene id before touching the model).
  let bgmSceneId = currentScene.id;
  $effect(() => {
    if (currentScene.id === bgmSceneId) return;
    bgmSceneId = currentScene.id;
    pendingBgmSlots = new Set();
    bgmErrors = {};
    draftBgmVolumes = {};
  });

  /**
   * Section-level error of the BGM block (Issue #275).
   *
   * The per-slot errors are rendered inside the row of the slot, but a row only
   * exists once its track write succeeded (Issue #249). Every failure of the
   * add flow (prepare / tinId-less asset / denied / rejected write) therefore
   * belongs to a slot that has **no** row, which is exactly the slot this
   * derives: the first error message of a slot that is not in `bgmSlots`,
   * including the user-facing error the audio manager recorded (missing room
   * key / download failure). One paragraph is enough: the add flow only ever
   * targets one slot at a time, and a second message would just repeat the same
   * cause.
   */
  let bgmAddError = $derived.by((): string | null => {
    const rowSlots = new Set(bgmSlots.map((slot) => slot.slot));
    const slotNumbers = [
      ...Object.keys(bgmErrors).map((key) => Number(key)),
      ...Object.keys(bgmAudioStates).map((key) => Number(key)),
    ].sort((a, b) => a - b);
    for (const slotNum of slotNumbers) {
      if (rowSlots.has(slotNum)) continue;
      const message = bgmErrors[slotNum] ?? bgmAudioStates[slotNum]?.error;
      if (message) return message;
    }
    return null;
  });

  function syncSlotToLegacy(slot: BgmSlot) {
    if (slot.slot === 1) {
      currentScene.juices.bgm1 = slot.tinId ?? undefined;
      currentScene.juices.bgm1Volume = slot.volume;
    } else if (slot.slot === 2) {
      currentScene.juices.bgm2 = slot.tinId ?? undefined;
      currentScene.juices.bgm2Volume = slot.volume;
    } else if (slot.slot === 3) {
      currentScene.juices.bgm3 = slot.tinId ?? undefined;
      currentScene.juices.bgm3Volume = slot.volume;
    }
  }

  function findBgmSlot(slotNum: number): BgmSlot | undefined {
    return bgmSlots.find((s) => s.slot === slotNum);
  }

  /** True when the slot is audible (playing intent plus a selected track). */
  function isBgmPlaying(slot: BgmSlot): boolean {
    return Boolean(slot.isPlaying && slot.tinId);
  }

  /** Converts a slot's state fields into the audio manager track shape. */
  function getSlotTrack(slot: BgmSlot | undefined): BgmTrackState | null {
    if (!slot?.tinId) return null;
    return buildBgmTrackState({
      tinId: slot.tinId,
      mimeType: slot.mimeType,
      name: slot.name,
    });
  }

  function isBgmPending(slotNum: number): boolean {
    return pendingBgmSlots.has(slotNum);
  }

  /**
   * True when the slot's controls must not be used (Issue #250).
   *
   * A viewer / denied role is read-only: it still hears the BGM another client
   * started, but it cannot write the transport state (the server would reject
   * the write with `PermissionDenied`).
   */
  function isBgmLocked(slotNum: number): boolean {
    return isBgmPending(slotNum) || !capabilities.canSend;
  }

  function setBgmPending(slotNum: number, pending: boolean) {
    const next = new Set(pendingBgmSlots);
    if (pending) {
      next.add(slotNum);
    } else {
      next.delete(slotNum);
    }
    pendingBgmSlots = next;
  }

  function setBgmError(slotNum: number, message: string | null) {
    const next = { ...bgmErrors };
    if (message) {
      next[slotNum] = message;
    } else {
      delete next[slotNum];
    }
    bgmErrors = next;
  }

  /** Legacy `bgm1..3` field of a slot, used when no slot state was remembered. */
  function getLegacyBgmTinId(slotNum: number): string | undefined {
    if (slotNum === 1) return currentScene.juices.bgm1;
    if (slotNum === 2) return currentScene.juices.bgm2;
    if (slotNum === 3) return currentScene.juices.bgm3;
    return undefined;
  }

  /** Legacy `bgm1..3` track as a playable reference (mime type fallback). */
  function getLegacyBgmTrack(slotNum: number): BgmTrackState | null {
    const tinId = getLegacyBgmTinId(slotNum)?.trim().toLowerCase();
    if (!tinId || !TIN_ID_HEX.test(tinId)) return null;
    return { tinId, mimeType: BGM_AUDIO_MIME_FALLBACK, name: '' };
  }

  /**
   * Transport timestamps committed with a playback state change (Issue #250).
   *
   * An omitted field keeps the existing row value; an explicit `null` clears it
   * (resuming deletes `paused_at`).
   */
  interface BgmTransportPatch {
    startedAt?: number | null;
    pausedAt?: number | null;
  }

  /** Applies a transport patch to a slot row field. */
  function patchedBgmTimestamp(
    patch: BgmTransportPatch,
    field: 'startedAt' | 'pausedAt',
    existing: number | undefined,
  ): number | undefined {
    if (!(field in patch)) return existing;
    const value = patch[field];
    return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
  }

  /**
   * Commits a track / playback change to the local scene after the server
   * acknowledged it.
   *
   * A track creates the row when the slot has none yet (Issue #249): the row is
   * the only representation of a selected track, so "add slot" needs no empty
   * placeholder and the row appears with this ack. The transport timestamps are
   * committed with it and are what every client derives its playback position
   * from (Issue #250). Called exactly once per successful operation;
   * `onUpdateScene` follows from it.
   */
  function applyBgmSlotState(
    slotNum: number,
    track: BgmTrackState | null,
    isPlaying: boolean,
    transport: BgmTransportPatch = {},
  ) {
    const existing = findBgmSlot(slotNum);
    const startedAt = patchedBgmTimestamp(
      transport,
      'startedAt',
      existing?.startedAt,
    );
    const pausedAt = patchedBgmTimestamp(transport, 'pausedAt', existing?.pausedAt);
    if (!track) {
      // A malformed / missing legacy track keeps the row and only updates the
      // playback flag; row removal is `applyBgmSlotRemoval` alone.
      bgmSlots = bgmSlots.map((s) =>
        s.slot === slotNum ? { ...s, isPlaying, startedAt, pausedAt } : s,
      );
    } else {
      const updated: BgmSlot = {
        slot: slotNum,
        tinId: track.tinId,
        mimeType: track.mimeType,
        name: track.name,
        volume: draftBgmVolumes[slotNum] ?? existing?.volume ?? 0.8,
        isPlaying,
        startedAt,
        pausedAt,
      };
      bgmSlots = [...bgmSlots.filter((s) => s.slot !== slotNum), updated].sort(
        (a, b) => a.slot - b.slot,
      );
    }
    const committed = findBgmSlot(slotNum);
    if (committed) {
      syncSlotToLegacy(committed);
    }
    currentScene.juices.bgmSlots = [...bgmSlots];
    onUpdateScene?.(currentScene);
  }

  /** Commits a volume change to the local scene after the server acknowledged it. */
  function applyBgmSlotVolume(slotNum: number, volume: number) {
    bgmSlots = bgmSlots.map((s) => (s.slot === slotNum ? { ...s, volume } : s));
    const updated = findBgmSlot(slotNum);
    if (updated) {
      syncSlotToLegacy(updated);
    }
    currentScene.juices.bgmSlots = [...bgmSlots];
    onUpdateScene?.(currentScene);
    const nextDrafts = { ...draftBgmVolumes };
    delete nextDrafts[slotNum];
    draftBgmVolumes = nextDrafts;
  }

  /**
   * Reports a failed BGM operation: reverts nothing locally (the state was never
   * applied) and shows the shared communication error next to the slot.
   */
  function reportBgmFailure(slotNum: number, error: unknown) {
    if (error instanceof ServerOperationError && error.code === 'PermissionDenied') {
      // The root scene `/` only lets administrators change state; keep the UI alive.
      console.warn(
        `[SceneSettingsTab] BGM operation for slot ${slotNum} was denied by the server (PermissionDenied)`,
        error,
      );
    } else {
      console.warn(`[SceneSettingsTab] BGM operation for slot ${slotNum} failed`, error);
    }
    setBgmError(slotNum, BGM_COMMUNICATION_ERROR);
  }

  /** Resolves once every request succeeded; rethrows the first failure. */
  async function throwIfBgmFailure(requests: Promise<unknown>[]): Promise<void> {
    const results = await Promise.allSettled(requests);
    const failure = results.find(
      (result): result is PromiseRejectedResult => result.status === 'rejected',
    );
    if (failure) throw failure.reason;
  }

  /**
   * Whether this instance can actually play audio.
   *
   * `roomKey === undefined` means the key was not resolved (standalone usage,
   * tests, previews); `null` means a room exists but has no key and is a real
   * playback error.
   */
  function canUseBgmAudio(): boolean {
    return !isStandalone() && roomKey !== undefined;
  }

  /**
   * Prepares the audio element before any state is written.
   *
   * A missing / undecryptable tin must never leave the server thinking the slot
   * is playing: the manager records the user-facing error and this returns
   * `false` so the caller skips the acknowledgement path. The message is also
   * copied into {@link bgmErrors} (Issue #275): the failure of a new slot has no
   * row to be rendered under, so the section-level error area has to learn about
   * it explicitly. The manager's own message (missing room key / load failure)
   * is preferred so the user sees the actual cause.
   */
  async function prepareBgmAudio(
    slotNum: number,
    track: BgmTrackState,
  ): Promise<boolean> {
    if (!canUseBgmAudio()) return true;
    // Apply the committed (or drafted) volume before the first playback.
    const volume =
      draftBgmVolumes[slotNum] ?? findBgmSlot(slotNum)?.volume ?? 0.8;
    bgmAudio.setVolume(slotNum, volume);
    try {
      await bgmAudio.prepare(slotNum, track, roomKey);
      return true;
    } catch (error) {
      console.warn(`[SceneSettingsTab] Failed to prepare BGM slot ${slotNum}:`, error);
      setBgmError(slotNum, bgmAudio.getState(slotNum).error ?? BGM_AUDIO_LOAD_ERROR);
      return false;
    }
  }

  /**
   * Starts the prepared element at a shared position (Issue #250).
   *
   * A browser autoplay refusal is not an error (`play()` resolves `'paused'`);
   * the next user gesture retries the same element from the same offset.
   */
  async function playBgmAudio(slotNum: number, offsetMs?: number): Promise<void> {
    if (!canUseBgmAudio()) return;
    try {
      await bgmAudio.play(slotNum, offsetMs);
    } catch (error) {
      console.warn(`[SceneSettingsTab] Failed to play BGM slot ${slotNum}:`, error);
    }
  }

  /** Wall-clock "now" used for the transport timestamps (`started_at` / `paused_at`). */
  function bgmNowMs(): number {
    return Date.now();
  }

  /**
   * `started_at` of a resume (Issue #250).
   *
   * `started_at = now - (paused_at - started_at)`, so the elapsed part of the
   * paused playback is kept and every client continues at the paused position.
   * The elapsed time is clamped at `>= 0`; a legacy paused slot without
   * `started_at` restarts from 0.
   */
  function resumeBgmStartedAt(slot: BgmSlot | undefined, now: number): number {
    const pausedAt = slot?.pausedAt;
    if (typeof pausedAt !== 'number' || !Number.isFinite(pausedAt)) return now;
    const startedAt = slot?.startedAt;
    const elapsed =
      typeof startedAt === 'number' && Number.isFinite(startedAt)
        ? Math.max(0, pausedAt - startedAt)
        : 0;
    return now - elapsed;
  }

  /** Position (ms) a playing slot starts at, derived from its `started_at`. */
  function bgmPlayOffsetMs(slotNum: number, startedAt: number): number {
    return computeBgmOffsetMs({
      now: bgmNowMs(),
      startedAt,
      durationMs: bgmAudio.getDurationMs(slotNum),
    });
  }

  /**
   * Plays a slot and commits `bgm::<slot>::playing = true` after the acks
   * (Issue #250).
   *
   * The state writes are acknowledged in order — track (when it changed) →
   * `started_at` → delete `paused_at` → `playing=true` — so no client observes
   * "playing" without a position. A paused slot resumes from its paused
   * position; a new selection / a slot that never started begins at 0.
   * `trackChanged` also writes `std::bgm<N>` as `{ tinId, mimeType, name }`.
   * Standalone usage (tests / previews) keeps the local-only path.
   */
  async function commitBgmPlay(
    slotNum: number,
    track: BgmTrackState,
    trackChanged: boolean,
  ) {
    if (isBgmPending(slotNum)) return;

    const socket = roomSocket;
    const sceneId = currentScene.id;
    const slot = findBgmSlot(slotNum);
    // Resume keeps the paused position; a new selection / a fresh play starts
    // the timeline now (offset 0).
    const now = bgmNowMs();
    const isResume = !trackChanged && slot?.pausedAt !== undefined;
    const startedAt = isResume ? resumeBgmStartedAt(slot, now) : now;
    setBgmError(slotNum, null);
    setBgmPending(slotNum, true);
    try {
      if (!(await prepareBgmAudio(slotNum, track))) {
        // No state write: the slot must not become "playing" without audio.
        return;
      }

      if (!socket || !socket.isConnected) {
        if (!isStandalone()) {
          setBgmError(slotNum, BGM_COMMUNICATION_ERROR);
          bgmAudio.pause(slotNum);
          return;
        }
        applyBgmSlotState(slotNum, track, true, { startedAt, pausedAt: null });
        await playBgmAudio(slotNum, bgmPlayOffsetMs(slotNum, startedAt));
        return;
      }

      const trackState = buildBgmTrackState(track);
      if (!trackState) {
        setBgmError(slotNum, BGM_MISSING_TIN_ERROR);
        bgmAudio.pause(slotNum);
        return;
      }

      // Sequential acks: the timestamp must be committed before the playback
      // intent, and `paused_at` must be gone before the slot is "playing".
      if (trackChanged) {
        await socket.requestUpdateState(
          sceneId,
          bgmTrackStateKey(slotNum),
          trackState,
        );
      }
      await socket.requestUpdateState(
        sceneId,
        bgmStartedAtStateKey(slotNum),
        startedAt,
      );
      await socket.requestDeleteState(sceneId, bgmPausedAtStateKey(slotNum));
      await socket.requestUpdateState(sceneId, bgmPlayingStateKey(slotNum), true);

      // The user may have switched scenes while the requests were in flight.
      if (currentScene.id !== sceneId) {
        bgmAudio.pause(slotNum);
        return;
      }
      applyBgmSlotState(slotNum, track, true, { startedAt, pausedAt: null });
      await playBgmAudio(slotNum, bgmPlayOffsetMs(slotNum, startedAt));
    } catch (error) {
      reportBgmFailure(slotNum, error);
      bgmAudio.pause(slotNum);
    } finally {
      setBgmPending(slotNum, false);
    }
  }

  /**
   * Pauses a slot and commits `bgm::<slot>::playing = false` after the acks
   * (Issue #250).
   *
   * `paused_at = now` is written first so every client freezes the same
   * position; the selected track stays in `std::bgm<N>` and the prepared audio
   * source is kept, so resuming continues at the paused position without
   * downloading the tin again. Only selecting NONE / 🗑️ deletes the track.
   */
  async function commitBgmStop(slotNum: number) {
    if (isBgmPending(slotNum)) return;

    const socket = roomSocket;
    const sceneId = currentScene.id;
    const track =
      getSlotTrack(findBgmSlot(slotNum)) ?? getLegacyBgmTrack(slotNum);
    const pausedAt = bgmNowMs();
    setBgmError(slotNum, null);
    setBgmPending(slotNum, true);
    try {
      if (!socket || !socket.isConnected) {
        if (!isStandalone()) {
          setBgmError(slotNum, BGM_COMMUNICATION_ERROR);
          return;
        }
        applyBgmSlotState(slotNum, track, false, { pausedAt });
        bgmAudio.pause(slotNum);
        return;
      }

      // Freeze the position before the playback intent flips (Issue #250).
      await socket.requestUpdateState(
        sceneId,
        bgmPausedAtStateKey(slotNum),
        pausedAt,
      );
      await socket.requestUpdateState(sceneId, bgmPlayingStateKey(slotNum), false);
      if (currentScene.id !== sceneId) return;
      applyBgmSlotState(slotNum, track, false, { pausedAt });
      bgmAudio.pause(slotNum);
    } catch (error) {
      reportBgmFailure(slotNum, error);
    } finally {
      setBgmPending(slotNum, false);
    }
  }

  /**
   * Restarts a slot from 0 on every client (⏪, Issue #250).
   *
   * `started_at = now` and `paused_at` is deleted, so every client derives
   * offset 0; the playback intent becomes `playing=true` when the slot was
   * paused. The element of this client restarts immediately.
   */
  async function commitBgmRestart(slotNum: number) {
    if (isBgmPending(slotNum)) return;

    const socket = roomSocket;
    const sceneId = currentScene.id;
    const slot = findBgmSlot(slotNum);
    const track = getSlotTrack(slot) ?? getLegacyBgmTrack(slotNum);
    if (!track) {
      // The row is shown for a legacy note-like value that cannot be played; a
      // restart cannot invent a source for it.
      setBgmError(slotNum, BGM_MISSING_TIN_ERROR);
      return;
    }
    const wasPlaying = Boolean(slot?.isPlaying);
    const startedAt = bgmNowMs();
    setBgmError(slotNum, null);
    setBgmPending(slotNum, true);
    try {
      if (!(await prepareBgmAudio(slotNum, track))) return;

      if (!socket || !socket.isConnected) {
        if (!isStandalone()) {
          setBgmError(slotNum, BGM_COMMUNICATION_ERROR);
          bgmAudio.pause(slotNum);
          return;
        }
        applyBgmSlotState(slotNum, track, true, { startedAt, pausedAt: null });
        await playBgmAudio(slotNum, 0);
        return;
      }

      await socket.requestUpdateState(
        sceneId,
        bgmStartedAtStateKey(slotNum),
        startedAt,
      );
      await socket.requestDeleteState(sceneId, bgmPausedAtStateKey(slotNum));
      if (!wasPlaying) {
        await socket.requestUpdateState(
          sceneId,
          bgmPlayingStateKey(slotNum),
          true,
        );
      }
      if (currentScene.id !== sceneId) {
        bgmAudio.pause(slotNum);
        return;
      }
      applyBgmSlotState(slotNum, track, true, { startedAt, pausedAt: null });
      await playBgmAudio(slotNum, 0);
    } catch (error) {
      reportBgmFailure(slotNum, error);
      bgmAudio.pause(slotNum);
    } finally {
      setBgmPending(slotNum, false);
    }
  }

  /**
   * Clears the legacy `bgm1..3` mirror of a deleted slot (Issue #226).
   *
   * `getNormalizedBgmSlots` falls back to the legacy fields when `bgmSlots` is
   * absent, so leaving the tin id behind would resurrect the deleted track on a
   * reload / after a scene switch.
   */
  function clearLegacyBgmSlot(slotNum: number) {
    if (slotNum === 1) {
      currentScene.juices.bgm1 = undefined;
      currentScene.juices.bgm1Volume = 0.8;
    } else if (slotNum === 2) {
      currentScene.juices.bgm2 = undefined;
      currentScene.juices.bgm2Volume = 0.8;
    } else if (slotNum === 3) {
      currentScene.juices.bgm3 = undefined;
      currentScene.juices.bgm3Volume = 0.8;
    }
  }

  /**
   * Removes a slot locally after every delete ack succeeded (Issue #226 / #249).
   *
   * The row disappears because it has no track any more (Issue #249); the other
   * rows keep their number, track, volume and playback state. The removal is
   * recorded in `juices.bgmRemovedSlots` (mirroring the server
   * `bgm::<slot>::removed` tombstone) so later broadcasts cannot re-create the
   * row. "スロット追加" uses max + 1 over rows and tombstones, so a deleted slot
   * number is never reused. The audio source is disposed (not just paused): the
   * slot, and with it the track timeline, no longer exists (Issue #250).
   */
  function applyBgmSlotRemoval(slotNum: number) {
    setBgmSlotRemoved(currentScene.juices, slotNum, true);
    bgmSlots = bgmSlots.filter((slot) => slot.slot !== slotNum);
    clearLegacyBgmSlot(slotNum);
    currentScene.juices.bgmSlots = [...bgmSlots];
    const nextDrafts = { ...draftBgmVolumes };
    delete nextDrafts[slotNum];
    draftBgmVolumes = nextDrafts;
    bgmAudio.stop(slotNum);
    onUpdateScene?.(currentScene);
  }

  /**
   * Restores the playback the deletion paused, but only when the delete failed
   * and the local model still says the slot is playing (Issue #226).
   *
   * The prepared source is still there (the deletion only paused it), so this
   * resumes at the slot's derived position without re-downloading (Issue #250).
   */
  async function restoreBgmPlaybackAfterFailedDelete(
    slotNum: number,
    wasPlaying: boolean,
    sceneId: string,
  ) {
    if (!wasPlaying || currentScene.id !== sceneId) return;
    const slot = findBgmSlot(slotNum);
    const track = getSlotTrack(slot) ?? getLegacyBgmTrack(slotNum);
    if (track && (await prepareBgmAudio(slotNum, track))) {
      const startedAt = slot?.startedAt ?? bgmNowMs();
      await playBgmAudio(slotNum, bgmPlayOffsetMs(slotNum, startedAt));
    }
  }

  /**
   * Deletes a BGM slot (Issue #226 / #250).
   *
   * The Banquette state becomes NONE for the slot first: the track, the volume
   * and the transport timestamps (`started_at` / `paused_at`) are deleted and
   * `bgm::<slot>::playing` is set to `false`. Only after all of those acks
   * succeed is the persistent `bgm::<slot>::removed` marker written, so a failed
   * NONE write never hides the slot behind a marker. The row is removed after
   * the marker ack; on failure the row and the local model stay untouched and
   * the shared communication error is shown.
   */
  async function commitBgmSlotDelete(slotNum: number) {
    if (isBgmPending(slotNum)) return;

    const socket = roomSocket;
    const sceneId = currentScene.id;
    const wasPlaying = Boolean(findBgmSlot(slotNum)?.isPlaying);
    // The user committed to deleting the slot: stop the sound right away, but
    // keep the source / row / model until the server acknowledged every write so
    // a failed deletion can resume without downloading again.
    bgmAudio.pause(slotNum);
    setBgmError(slotNum, null);
    setBgmPending(slotNum, true);
    try {
      if (!socket || !socket.isConnected) {
        if (!isStandalone()) {
          setBgmError(slotNum, BGM_COMMUNICATION_ERROR);
          await restoreBgmPlaybackAfterFailedDelete(slotNum, wasPlaying, sceneId);
          return;
        }
        // Standalone usage (tests / previews): no room to write to.
        applyBgmSlotRemoval(slotNum);
        return;
      }

      await throwIfBgmFailure([
        socket.requestDeleteState(sceneId, bgmTrackStateKey(slotNum)),
        socket.requestDeleteState(sceneId, bgmStartedAtStateKey(slotNum)),
        socket.requestDeleteState(sceneId, bgmPausedAtStateKey(slotNum)),
        socket.requestUpdateState(sceneId, bgmPlayingStateKey(slotNum), false),
        socket.requestDeleteState(sceneId, bgmVolumeStateKey(slotNum)),
      ]);
      if (currentScene.id !== sceneId) return;

      // Marker last, and as a separate step: one of the NONE writes above may
      // have failed, in which case the slot must stay visible / retryable.
      await socket.requestUpdateState(sceneId, bgmRemovedStateKey(slotNum), true);
      if (currentScene.id !== sceneId) return;
      applyBgmSlotRemoval(slotNum);
    } catch (error) {
      reportBgmFailure(slotNum, error);
      await restoreBgmPlaybackAfterFailedDelete(slotNum, wasPlaying, sceneId);
    } finally {
      setBgmPending(slotNum, false);
    }
  }

  function handleDeleteBgmSlot(slotNum: number) {
    if (isBgmPending(slotNum)) return;
    void commitBgmSlotDelete(slotNum);
  }

  function handleToggleBgmPlay(slotNum: number) {
    if (isBgmPending(slotNum)) return;
    const slot = findBgmSlot(slotNum);
    if (!slot) return;

    if (isBgmPlaying(slot)) {
      void commitBgmStop(slotNum);
      return;
    }

    // 停止中 -> 再生: the selected track is kept while stopped. A track that
    // only exists in the legacy `bgm1..3` field is migrated to `std::bgm<N>`.
    const slotTrack = getSlotTrack(slot);
    const track = slotTrack ?? getLegacyBgmTrack(slotNum);
    if (!track) {
      // 音源がなければアセット選択を開く
      handleBgmSelect(slotNum);
      return;
    }
    void commitBgmPlay(slotNum, track, slotTrack === null);
  }

  /** ⏪: restart every client at 0 via `started_at = now` (Issue #250). */
  function handleRestartBgm(slotNum: number) {
    if (isBgmPending(slotNum)) return;
    void commitBgmRestart(slotNum);
  }

  /**
   * Opens the AssetPicker for a slot (Issue #275: read-only roles never start
   * the flow — the server would reject the writes with `PermissionDenied` and a
   * new slot has no row to show that error under).
   */
  function handleBgmSelect(slotOrKey: number | 'bgm1' | 'bgm2' | 'bgm3') {
    if (!capabilities.canSend) return;
    const slotNum =
      typeof slotOrKey === 'number'
        ? slotOrKey
        : slotOrKey === 'bgm1'
          ? 1
          : slotOrKey === 'bgm2'
            ? 2
            : 3;
    if (isBgmPending(slotNum)) return;
    activeBgmSlotPicking = slotNum;
    assetPickerTarget = `bgm${slotNum}` as any;
    assetPickerMediaType = 'audio';
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'audio',
        initialScope: 'personal',
        onSelect: (dataUrl, asset) => {
          handleSlotAssetPicked(slotNum, dataUrl, asset);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  /**
   * Handles a picked audio asset.
   *
   * The room state stores the cellar track reference (`{ tinId, mimeType,
   * name }`), never the local data URL. Assets without a tin id (legacy
   * local-only entries) are rejected with an inline error instead of writing an
   * unsynchronisable value. Selecting NONE (no data URL and no asset) deletes
   * the slot exactly like the 🗑️ button (Issue #249).
   */
  function handleSlotAssetPicked(slotNum: number, dataUrl?: string, asset?: AssetItem) {
    activeBgmSlotPicking = null;
    if (isBgmPending(slotNum)) return;

    if (!dataUrl && !asset) {
      void commitBgmSlotDelete(slotNum);
      return;
    }

    const tinId =
      typeof asset?.tinId === 'string' ? asset.tinId.trim().toLowerCase() : '';
    if (!TIN_ID_HEX.test(tinId)) {
      console.warn(
        `[SceneSettingsTab] The selected audio asset has no cellar tin id; slot ${slotNum} keeps its previous track.`,
      );
      setBgmError(slotNum, BGM_MISSING_TIN_ERROR);
      return;
    }
    // `dataUrl` is only useful for local preview; the authoritative room state
    // value is the tin reference, so it is deliberately not persisted here.
    void commitBgmPlay(
      slotNum,
      {
        tinId,
        mimeType:
          typeof asset?.mimeType === 'string' && asset.mimeType.startsWith('audio/')
            ? asset.mimeType
            : BGM_AUDIO_MIME_FALLBACK,
        name: typeof asset?.name === 'string' ? asset.name : '',
      },
      true,
    );
  }

  function readBgmSliderValue(e: Event): number {
    const target = e.target as HTMLInputElement | null;
    const raw = (e as any).detail?.value ?? Number(target?.value);
    const hundred = Number.isFinite(raw) ? raw : 0;
    return Math.max(0, Math.min(100, hundred)) / 100;
  }

  /**
   * Drag preview: updates the label and the live audio element volume.
   * Nothing is sent to the server until the slider commit.
   */
  function handleBgmVolumePreview(slotNum: number, volume: number) {
    draftBgmVolumes = { ...draftBgmVolumes, [slotNum]: volume };
    bgmAudio.setVolume(slotNum, volume);
  }

  /** Committed volume change: `input` ticks never reach the server. */
  function handleBgmVolumeCommit(slotNum: number, volume: number) {
    if (isBgmPending(slotNum)) return;
    draftBgmVolumes = { ...draftBgmVolumes, [slotNum]: volume };
    bgmAudio.setVolume(slotNum, volume);
    setBgmError(slotNum, null);

    const socket = roomSocket;
    const sceneId = currentScene.id;
    if (!socket || !socket.isConnected) {
      applyBgmSlotVolume(slotNum, volume);
      return;
    }

    void (async () => {
      setBgmPending(slotNum, true);
      try {
        await socket.requestUpdateState(sceneId, bgmVolumeStateKey(slotNum), volume);
        if (currentScene.id !== sceneId) return;
        applyBgmSlotVolume(slotNum, volume);
      } catch (error) {
        reportBgmFailure(slotNum, error);
        // Drop the preview so the slider and the audio element show the last
        // acknowledged volume.
        if (currentScene.id === sceneId) {
          const nextDrafts = { ...draftBgmVolumes };
          delete nextDrafts[slotNum];
          draftBgmVolumes = nextDrafts;
          const committed = findBgmSlot(slotNum)?.volume;
          if (typeof committed === 'number') bgmAudio.setVolume(slotNum, committed);
        }
      } finally {
        setBgmPending(slotNum, false);
      }
    })();
  }

  /**
   * "スロット追加": opens the AssetPicker for the next slot number (Issue #249).
   *
   * The next number is `max(existing rows ∪ removed tombstones) + 1` (1 when
   * the scene has neither), so a deleted slot number is never reused and its
   * `bgm::<slot>::removed` tombstone never hides the new selection. No empty
   * row is created here: the row appears only after the selection is
   * acknowledged, through the track StateUpdated / the ack of `commitBgmPlay`.
   * A viewer / denied role cannot use it at all (Issue #275): the button is
   * disabled with the same capability predicate as the row controls.
   */
  function handleAddBgmSlot() {
    if (!capabilities.canSend) return;
    const maxRow = bgmSlots.reduce((max, slot) => Math.max(max, slot.slot), 0);
    const maxRemoved = getBgmRemovedSlots(currentScene.juices).reduce(
      (max, slot) => Math.max(max, slot),
      0,
    );
    handleBgmSelect(Math.max(maxRow, maxRemoved) + 1);
  }

  /**
   * Publishes a picked background image to the cellar and the scene state
   * (Issue #159).
   *
   * The local preview changes only after the upload and the state
   * acknowledgements succeed; any failure keeps the previous background and
   * shows the communication error. Standalone usage (no room socket and no
   * resolved room key) keeps the legacy local-only behaviour for tests.
   */
  async function commitBackgroundImage(dataUrl?: string, asset?: AssetItem) {
    const sceneId = currentScene.id;
    const socket = roomSocket;
    bgImageError = null;

    if (!dataUrl) {
      if (!socket || !socket.isConnected) {
        if (isStandalone()) {
          currentScene.juices.bgImage = undefined;
          onUpdateScene?.(currentScene);
        } else {
          bgImageError = SCENE_STATE_COMMUNICATION_ERROR;
        }
        return;
      }
      try {
        await clearSceneBackground({ sceneId, roomSocket: socket });
        if (currentScene.id !== sceneId) return;
        currentScene.juices.bgImage = undefined;
        onUpdateScene?.(currentScene);
      } catch (error) {
        console.warn('[SceneSettingsTab] Failed to clear the background image:', error);
        bgImageError = SCENE_STATE_COMMUNICATION_ERROR;
      }
      return;
    }

    if (!socket || !socket.isConnected) {
      if (isStandalone()) {
        currentScene.juices.bgImage = dataUrl;
        onUpdateScene?.(currentScene);
      } else {
        bgImageError = SCENE_STATE_COMMUNICATION_ERROR;
      }
      return;
    }
    if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
      bgImageError = SCENE_BACKGROUND_MISSING_KEY_ERROR;
      return;
    }

    bgImagePending = true;
    try {
      const result = await uploadSceneBackground({
        sceneId,
        dataUrl,
        asset,
        roomKey,
        roomSocket: socket,
        width: currentScene.juices.bgWidth,
        height: currentScene.juices.bgHeight,
      });
      if (currentScene.id !== sceneId) return;
      // The state stores the tin id; the local preview is the picked data URL.
      currentScene.juices.bgImage = result.dataUrl;
      onUpdateScene?.(currentScene);
    } catch (error) {
      console.warn('[SceneSettingsTab] Failed to upload the background image:', error);
      bgImageError = SCENE_STATE_COMMUNICATION_ERROR;
    } finally {
      bgImagePending = false;
    }
  }

  function handleAssetPicked(dataUrl?: string, asset?: AssetItem) {
    if (assetPickerTarget === 'bgImage') {
      void commitBackgroundImage(dataUrl, asset);
    } else if (activeBgmSlotPicking !== null) {
      const slotNum = activeBgmSlotPicking;
      activeBgmSlotPicking = null;
      // BGM applies (and notifies `onUpdateScene`) itself, once the server acks.
      handleSlotAssetPicked(slotNum, dataUrl, asset);
    } else {
      currentScene.juices[assetPickerTarget] = dataUrl;
      onUpdateScene?.(currentScene);
    }
  }

  function handleQuickSelectBgImage(dataUrl: string | undefined) {
    void commitBackgroundImage(dataUrl);
  }

  /**
   * Commits a scene rename after the server acknowledged `std::name`.
   *
   * Standalone usage updates the local model directly; in a room the local
   * name changes only after the `StateUpdated` ack, so a failed rename never
   * looks applied.
   */
  async function commitSceneName(rawName: string) {
    const name = rawName.trim();
    const sceneId = currentScene.id;
    if (!name || name === currentScene.name) return;
    sceneNameError = null;

    const socket = roomSocket;
    if (!socket || !socket.isConnected) {
      if (isStandalone()) {
        currentScene.name = name;
        onUpdateScene?.(currentScene);
      } else {
        sceneNameError = SCENE_STATE_COMMUNICATION_ERROR;
      }
      return;
    }

    try {
      await socket.requestUpdateState(sceneId, SCENE_NAME_STATE_KEY, name);
      if (currentScene.id !== sceneId) return;
      currentScene.name = name;
      sceneNameDraft = name;
      onUpdateScene?.(currentScene);
    } catch (error) {
      console.warn('[SceneSettingsTab] Failed to rename the scene:', error);
      sceneNameError = SCENE_STATE_COMMUNICATION_ERROR;
      // The server rejected / never answered: show the committed name again.
      sceneNameDraft = currentScene.name;
    }
  }

  function handleSceneNameInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    sceneNameDraft = value;
    sceneNameError = null;

    if (sceneNameTimer) {
      clearTimeout(sceneNameTimer);
      sceneNameTimer = null;
    }

    if (isStandalone()) {
      currentScene.name = value;
      onUpdateScene?.(currentScene);
      return;
    }

    // Debounce so keystrokes do not each become a state round trip.
    sceneNameTimer = setTimeout(() => {
      sceneNameTimer = null;
      void commitSceneName(value);
    }, 400);
  }

  // ---------------------------------------------------------------------------
  // Management actions (Issue #109 / sub-issue #143)
  // ---------------------------------------------------------------------------
  /** Failure text shown centered under the management controls. */
  const SCENE_ACTION_COMMUNICATION_ERROR = 'サーバーと通信できませんでした';
  /**
   * Action this tab initiated itself. `App` owns the pending/error state for
   * the server round trip; this local mirror keeps the spinner and the error
   * working when the tab is used standalone (tests, storybook-like usage).
   */
  let localPendingAction = $state<string | null>(null);
  let localActionError = $state<string | null>(null);

  /** Pending action: `App`'s state wins, the local one covers local initiation. */
  let effectivePendingAction = $derived(sceneActionPending ?? localPendingAction);
  /** Failure text: `App`'s message wins, the local one covers `false` returns. */
  let effectiveActionError = $derived(sceneActionError || localActionError);

  function isSceneActionPending(actionId: string): boolean {
    return effectivePendingAction === actionId;
  }

  function isAnySceneActionPending(): boolean {
    return effectivePendingAction !== null;
  }

  /**
   * Runs an action initiated by this tab.
   *
   * The callback may be synchronous (legacy tests) or return a Promise. A
   * callback returning `false` — or rejecting — is a failure: the communication
   * error is shown and no local state is mutated. `App` also reports its own
   * failure through `sceneActionError`, so both paths show the same message.
   */
  async function runSceneAction(
    actionId: string,
    action: (() => Promise<boolean | void> | boolean | void) | undefined,
  ): Promise<void> {
    if (isAnySceneActionPending()) return;
    localActionError = null;
    localPendingAction = actionId;
    try {
      const result = await action?.();
      if (result === false) {
        localActionError = SCENE_ACTION_COMMUNICATION_ERROR;
      }
    } catch (error) {
      console.warn(`[SceneSettingsTab] Scene action "${actionId}" failed:`, error);
      localActionError = SCENE_ACTION_COMMUNICATION_ERROR;
    } finally {
      localPendingAction = null;
    }
  }

  async function handleCreateSceneSubmit() {
    const name = newSceneName.trim();
    if (!name || isAnySceneActionPending()) return;
    const inheritJuice = inheritJuiceToggle;
    // Close the dialog and clear the field immediately, like before, so the
    // spinner is visible on the management icon while the server answers.
    newSceneName = '';
    isCreateSceneOpen = false;
    await runSceneAction(
      'create',
      onCreateScene ? () => onCreateScene(name, inheritJuice) : undefined,
    );
  }

  function handleDeleteButtonClick() {
    if (onRequestDeleteScene) {
      onRequestDeleteScene({
        id: currentScene.id,
        name: currentScene.name,
      });
    } else {
      isDeleteConfirmOpen = true;
    }
  }

  async function handleDeleteConfirm() {
    isDeleteConfirmOpen = false;
    await runSceneAction(
      'delete-scene',
      onDeleteScene ? () => onDeleteScene(currentScene.id) : undefined,
    );
  }

  function handleDeleteLogsButtonClick() {
    if (onRequestDeleteSceneLogs) {
      onRequestDeleteSceneLogs({
        id: currentScene.id,
        name: currentScene.name,
      });
    } else {
      isDeleteLogsConfirmOpen = true;
    }
  }

  async function handleDeleteLogsConfirm() {
    isDeleteLogsConfirmOpen = false;
    await runSceneAction(
      'delete-logs',
      onDeleteSceneLogs ? () => onDeleteSceneLogs(currentScene.id) : undefined,
    );
  }
</script>

<div class="scene-settings-container" data-testid="scene-settings-tab">
  <div class="settings-scroll-content">
    <!-- Heading Juices -->
    <h3 class="section-heading" data-testid="heading-juices">Juices</h3>

    <div class="setting-item">
      <GenericImagePicker
        label="背景画像アセット"
        aspectRatio={16 / 9}
        showNone={true}
        isAdmin={capabilities.isAdmin}
        imageAssets={imageAssets}
        onSelect={handleQuickSelectBgImage}
        onOpenPicker={handleBgImageSelect}
        onRequestAssetPicker={onRequestAssetPicker}
        dataTestId="bg-generic-image-picker"
        dataCarouselTestId="bg-image-carousel"
        noneTestId="bg-carousel-item-none"
        itemTestIdPrefix="bg-carousel-item"
        moreButtonTestId="more-images-btn"
        cardTestId="bg-image-card"
        legacyOpenBtnTestId="open-bg-picker-btn"
      />
      <p class="description-text" data-testid="bg-description-text">
        画像は一方を指定すればもう一方を自動で計算します。もう一方も入力した場合、端を切り落として揃えます。
      </p>

      <div class="dimensions-row">
        <div class="dim-field">
          <TextField
            id="width-input"
            variant="outlined"
            type="number"
            label="横幅"
            placeholder="自動計算"
            suffixText="px"
            value={inputWidth ?? ''}
            oninput={handleWidthChange}
            data-testid="bg-width-input"
            class="dim-text-field"
          />
        </div>
        <div class="dim-field">
          <TextField
            id="height-input"
            variant="outlined"
            type="number"
            label="縦幅"
            placeholder="自動計算"
            suffixText="px"
            value={inputHeight ?? ''}
            oninput={handleHeightChange}
            data-testid="bg-height-input"
            class="dim-text-field"
          />
        </div>
      </div>
      {#if bgImagePending}
        <p class="bg-image-pending" role="status" data-testid="bg-image-pending">
          背景画像を保存しています…
        </p>
      {/if}
      {#if bgImageError}
        <p class="scene-state-error" role="status" data-testid="bg-image-error">
          {bgImageError}
        </p>
      {/if}
    </div>

    <!-- BGM Controls (Issue #53: Reconstructed with slot u32, toggle play/stop, narrow button, 1:1 select button, volume slider) -->
    <div class="bgm-controls" data-testid="bgm-controls">
      <div class="bgm-section-header">
        <span class="bgm-header-label">BGMスロット設定</span>
        <!-- Issue #275: a viewer / denied role is read-only, so it cannot start
             the add flow (the row controls use the same predicate). -->
        <button
          type="button"
          class="add-slot-text-btn"
          onclick={handleAddBgmSlot}
          disabled={!capabilities.canSend}
          data-testid="add-bgm-slot-btn"
          aria-label="スロットを追加"
        >
          <Icon>add</Icon>
          <span>スロット追加</span>
        </button>
      </div>

      <!-- Issue #275: 追加フローの失敗は行が存在しないスロットで起きるため、
           行の中 (`bgm-error-<slot>`) ではなく節の直下に 1 つだけ表示する。 -->
      {#if bgmAddError}
        <p class="bgm-add-error-text" role="status" data-testid="bgm-add-error">
          {bgmAddError}
        </p>
      {/if}

      {#each bgmSlots as slot (slot.slot)}
        <div class="bgm-slot-block" data-testid="bgm-slot-block-{slot.slot}">
          <div class="bgm-row" data-testid="bgm-row-{slot.slot}">
            <!-- トランスポート (Issue #250): ⏪ / ▶️・⏸️ / 🎵 を M3 の接続ボタングループに
                 まとめ、その右に音量、右端に削除を置く。 -->
            <ButtonGroup
              variant="connected"
              size="s"
              shape="round"
              aria-label="BGMトランスポート"
              data-testid="bgm-transport-group-{slot.slot}"
              class="bgm-transport-group"
            >
              <!-- 1. 最初から再生 (⏪): started_at を now にして全クライアントを 0 秒へ -->
              <IconButton
                variant="tonal"
                size="s"
                shape="round"
                aria-label="最初から再生"
                title={`スロット${slot.slot}: 最初から再生`}
                onclick={() => handleRestartBgm(slot.slot)}
                disabled={isBgmLocked(slot.slot)}
                data-testid="bgm-restart-btn-{slot.slot}"
                class="bgm-transport-btn bgm-restart-btn"
              >
                {#if isBgmPending(slot.slot)}
                  <CircularProgress
                    indeterminate
                    wavy
                    track={false}
                    class="bgm-pending-progress"
                    data-testid="bgm-pending-restart-{slot.slot}"
                    aria-label="送信中"
                  />
                {:else}
                  <Icon>replay</Icon>
                {/if}
              </IconButton>

              <!-- 2. 再生/一時停止 (▶️ / ⏸️): 一時停止は位置を保持し、再開は停止位置から -->
              <IconButton
                variant="tonal"
                size="s"
                width="narrow"
                shape="round"
                toggle={true}
                selected={isBgmPlaying(slot)}
                aria-label={isBgmPlaying(slot) ? 'BGM一時停止' : 'BGM再生'}
                aria-pressed={isBgmPlaying(slot) ? 'true' : 'false'}
                onclick={() => handleToggleBgmPlay(slot.slot)}
                disabled={isBgmLocked(slot.slot)}
                data-testid="bgm-play-btn-{slot.slot}"
                class="bgm-transport-btn bgm-play-btn narrow-btn"
                title={isBgmPlaying(slot)
                  ? `スロット${slot.slot}: 一時停止`
                  : `スロット${slot.slot}: 再生`}
              >
                {#if isBgmPending(slot.slot)}
                  <CircularProgress
                    indeterminate
                    wavy
                    track={false}
                    class="bgm-pending-progress"
                    data-testid="bgm-pending-play-{slot.slot}"
                    aria-label="送信中"
                  />
                {:else}
                  <Icon>{isBgmPlaying(slot) ? 'pause' : 'play_arrow'}</Icon>
                {/if}
              </IconButton>

              <!-- 3. 選択ボタン: M3 IconButton, music_note アイコン
                   送信中は波線の CircularProgress に差し替える (Issue #109 / #142) -->
              <IconButton
                variant="tonal"
                size="s"
                width="default"
                shape="round"
                aria-label={isBgmPending(slot.slot) ? 'BGM送信中' : 'BGM選択'}
                onclick={() => handleBgmSelect(slot.slot)}
                disabled={isBgmLocked(slot.slot)}
                data-testid="bgm-select-btn-{slot.slot}"
                class="bgm-transport-btn bgm-select-btn square-btn"
                title={isBgmPending(slot.slot)
                  ? `スロット${slot.slot}: 送信中`
                  : `スロット${slot.slot}: BGM選択`}
              >
                {#if isBgmPending(slot.slot)}
                  <CircularProgress
                    indeterminate
                    wavy
                    track={false}
                    class="bgm-pending-progress"
                    data-testid="bgm-pending-{slot.slot}"
                    aria-label="送信中"
                  />
                {:else}
                  <Icon>music_note</Icon>
                {/if}
              </IconButton>
            </ButtonGroup>

            <!-- 4. 音量バー: M3 Slider (0〜100)。確定 (change) 時だけ送信する -->
            <div class="slider-wrapper" data-testid="bgm-slider-wrapper-{slot.slot}">
              <Slider
                min={0}
                max={100}
                step={1}
                value={Math.round((draftBgmVolumes[slot.slot] ?? slot.volume ?? 0.8) * 100)}
                oninput={(e: Event) => {
                  handleBgmVolumePreview(slot.slot, readBgmSliderValue(e));
                }}
                onchange={(e: Event) => {
                  handleBgmVolumeCommit(slot.slot, readBgmSliderValue(e));
                }}
                disabled={isBgmLocked(slot.slot)}
                aria-label="BGM音量"
                data-testid="bgm-slider-{slot.slot}"
              />
              <span class="vol-label" data-testid="bgm-vol-label-{slot.slot}">
                {Math.round((draftBgmVolumes[slot.slot] ?? slot.volume ?? 0.8) * 100)}%
              </span>
            </div>

            <!-- 5. 削除ボタン: M3 Filled tonal IconButton (Issue #226)。
                 押下でスロットを Banquette から NONE にし、行を削除する。
                 アセットピッカーの NONE カードとは独立した操作。 -->
            <IconButton
              variant="tonal"
              size="s"
              shape="round"
              aria-label="BGMスロットを削除"
              title="BGMスロットを削除"
              onclick={() => handleDeleteBgmSlot(slot.slot)}
              disabled={isBgmLocked(slot.slot)}
              data-testid="bgm-delete-btn-{slot.slot}"
              class="bgm-delete-btn"
            >
              {#if isBgmPending(slot.slot)}
                <CircularProgress
                  indeterminate
                  wavy
                  track={false}
                  class="bgm-pending-progress"
                  data-testid="bgm-pending-delete-{slot.slot}"
                  aria-label="送信中"
                />
              {:else}
                <Icon>delete</Icon>
              {/if}
            </IconButton>
          </div>

          {#if bgmErrors[slot.slot] || bgmAudioStates[slot.slot]?.error}
            <p class="bgm-error-text" role="status" data-testid="bgm-error-{slot.slot}">
              {bgmErrors[slot.slot] ?? bgmAudioStates[slot.slot]?.error}
            </p>
          {/if}
        </div>
      {/each}
    </div>

    <!-- Heading 管理 -->
    <h3 class="section-heading" data-testid="heading-management">管理</h3>

    <!-- Management buttons in 1 row with 7 : 5 : 3 ratio and soft Material Color for delete -->
    <div class="management-buttons" data-testid="management-buttons">
      <!-- Scene Name Edit Field (Issue #123: Moved above create scene in management section) -->
      <div class="scene-name-container" data-testid="scene-name-container">
        <TextField
          variant="outlined"
          label="シーン名"
          value={sceneNameDraft}
          oninput={handleSceneNameInput}
          disabled={!canManageScenes}
          data-testid="scene-name-input"
          placeholder="シーン名"
          class="scene-name-field"
        />
        {#if sceneNameError}
          <p class="scene-state-error" role="status" data-testid="scene-name-error">
            {sceneNameError}
          </p>
        {/if}
      </div>

      <!-- 3つのアクション「シーンを新規作成」「シーンのログを削除」「シーンを削除」:
           横並びの Square L ボタン群として配置。
           初期状態では左ほど大きく、右ほど小さい square の比率。
           マウスホバー時に行全体における自身の占有率がスムーズに拡大し、アイコンの右横にラベル文字が展開表示される。
      -->
      <div
        class="scene-actions-row"
        class:has-hover={hoveredAction !== null}
        data-testid="scene-actions-row"
      >
        <!-- シーンを新規作成 -->
        <div
          class="action-btn-col action-col-create"
          class:hovered={hoveredAction === 'create'}
          style="flex: {hoveredAction === 'create' ? '10 1 0%' : hoveredAction ? '1.5 1 0%' : '5 1 0%'};"
          onmouseenter={() => (hoveredAction = 'create')}
          onmouseleave={() => (hoveredAction = null)}
          data-testid="action-col-create"
        >
          <Button
            variant="tonal"
            shape="square"
            size="m"
            class="action-square-btn create-scene-btn"
            onclick={() => (isCreateSceneOpen = true)}
            disabled={!canManageScenes || isAnySceneActionPending()}
            aria-label="シーンを新規作成"
            data-testid="open-create-scene-btn"
          >
            {#snippet start()}
              {#if isSceneActionPending('create')}
                <CircularProgress
                  indeterminate
                  wavy
                  track={false}
                  class="scene-action-progress"
                  data-testid="create-action-pending"
                  aria-label="送信中"
                />
              {:else}
                <Icon>add</Icon>
              {/if}
            {/snippet}
            <span class="action-btn-label" data-testid="create-scene-label">シーンを新規作成</span>
          </Button>
        </div>

        <!-- シーンのログを削除 -->
        <div
          class="action-btn-col action-col-delete-logs"
          class:hovered={hoveredAction === 'delete-logs'}
          style="flex: {hoveredAction === 'delete-logs' ? '10 1 0%' : hoveredAction ? '1.5 1 0%' : '3.2 1 0%'};"
          onmouseenter={() => (hoveredAction = 'delete-logs')}
          onmouseleave={() => (hoveredAction = null)}
          data-testid="action-col-delete-logs"
        >
          <Button
            variant="tonal"
            shape="square"
            size="m"
            class="action-square-btn delete-logs-btn destructive-tonal-btn"
            onclick={handleDeleteLogsButtonClick}
            disabled={!canManageScenes || isAnySceneActionPending()}
            aria-label="シーンのログを削除"
            data-testid="delete-logs-btn"
          >
            {#snippet start()}
              {#if isSceneActionPending('delete-logs')}
                <CircularProgress
                  indeterminate
                  wavy
                  track={false}
                  class="scene-action-progress"
                  data-testid="delete-logs-action-pending"
                  aria-label="送信中"
                />
              {:else}
                <Icon>delete_sweep</Icon>
              {/if}
            {/snippet}
            <span class="action-btn-label" data-testid="delete-logs-label">ログを削除</span>
          </Button>
        </div>

        <!-- シーンを削除 -->
        <div
          class="action-btn-col action-col-delete-scene"
          class:hovered={hoveredAction === 'delete-scene'}
          style="flex: {hoveredAction === 'delete-scene' ? '10 1 0%' : hoveredAction ? '1.5 1 0%' : '2.2 1 0%'};"
          onmouseenter={() => (hoveredAction = 'delete-scene')}
          onmouseleave={() => (hoveredAction = null)}
          data-testid="action-col-delete-scene"
        >
          <Button
            variant="tonal"
            shape="square"
            size="m"
            class="action-square-btn delete-scene-btn delete-scene-tonal-btn destructive-tonal-btn"
            onclick={handleDeleteButtonClick}
            disabled={!canManageScenes || isAnySceneActionPending()}
            aria-label="シーンを削除"
            data-testid="delete-scene-btn"
          >
            {#snippet start()}
              {#if isSceneActionPending('delete-scene')}
                <CircularProgress
                  indeterminate
                  wavy
                  track={false}
                  class="scene-action-progress"
                  data-testid="delete-scene-action-pending"
                  aria-label="送信中"
                />
              {:else}
                <Icon>delete</Icon>
              {/if}
            {/snippet}
            <span class="action-btn-label" data-testid="delete-scene-label">シーンを削除</span>
          </Button>
        </div>
      </div>

    </div>

    <!-- 管理操作の失敗表示: 管理コントロール群の下に中央寄せで出す (Issue #109 / #143) -->
    {#if effectiveActionError}
      <p class="scene-action-error" role="status" data-testid="scene-action-error">
        {effectiveActionError}
      </p>
    {/if}
  </div>

  <!-- Overlay: シーンを新規作成 -->
  {#if isCreateSceneOpen}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="create-scene-overlay"
      onclick={(e) => {
        if (e.target === e.currentTarget) isCreateSceneOpen = false;
      }}
    >
      <div class="dialog-box">
        <h2 class="dialog-title-centered" data-testid="create-scene-title">シーンを新規作成</h2>

        <div class="dialog-input-row">
          <TextField
            id="new-scene-name"
            variant="outlined"
            label="シーン名"
            placeholder="シーン名"
            bind:value={newSceneName}
            data-testid="new-scene-name-input"
            class="new-scene-name-field"
          />
        </div>

        <div class="toggle-row">
          <span class="label-text">Juicesを継承</span>
          <Switch
            bind:selected={inheritJuiceToggle}
            icons="selected"
            aria-label="Juicesを継承"
            data-testid="inherit-juices-toggle"
          />
        </div>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isCreateSceneOpen = false)}>キャンセル</Button>
          <Button variant="filled" onclick={handleCreateSceneSubmit} data-testid="submit-create-scene-btn">
            新規作成
          </Button>
        </div>
      </div>
    </div>
  {/if}



  <!-- Overlay: シーンを削除 Full-Screen Confirmation Dialog -->
  {#if isDeleteConfirmOpen}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="delete-confirm-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) isDeleteConfirmOpen = false;
      }}
    >
      <div class="dialog-box warning-box">
        <h2 class="dialog-title-centered warning-text" data-testid="delete-dialog-title">
          シーン「{currentScene.name}」を削除しますか？
        </h2>
        <p class="warning-body" data-testid="delete-dialog-warning">
          このシーンを削除すると、配置されたログやJuices設定は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isDeleteConfirmOpen = false)}>キャンセル</Button>
          <Button
            variant="tonal"
            class="delete-scene-tonal-btn"
            onclick={handleDeleteConfirm}
            data-testid="confirm-delete-button"
          >
            削除する
          </Button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Overlay: シーンのログを削除 Full-Screen Confirmation Dialog -->
  {#if isDeleteLogsConfirmOpen}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="delete-logs-confirm-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) isDeleteLogsConfirmOpen = false;
      }}
    >
      <div class="dialog-box warning-box">
        <h2 class="dialog-title-centered warning-text" data-testid="delete-logs-dialog-title">
          シーン「{currentScene.name}」のログを削除しますか？
        </h2>
        <p class="warning-body" data-testid="delete-logs-dialog-warning">
          このシーンのすべての会話ログを削除します。この操作は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isDeleteLogsConfirmOpen = false)} data-testid="cancel-delete-logs-btn">
            キャンセル
          </Button>
          <Button
            variant="tonal"
            class="delete-scene-tonal-btn destructive-tonal-btn"
            onclick={handleDeleteLogsConfirm}
            data-testid="confirm-delete-logs-button"
          >
            削除する
          </Button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Overlay: アセット選択 (AssetPickerModal) -->
  {#if isAssetPickerOpen}
    <AssetPickerModal
      open={isAssetPickerOpen}
      initialMediaType={assetPickerMediaType}
      isAdmin={capabilities.isAdmin}
      onSelect={handleAssetPicked}
      onClose={() => (isAssetPickerOpen = false)}
    />
  {/if}
</div>

<style>
  .scene-settings-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--np-color-surface);
    color: var(--np-color-on-surface);
  }
  .scene-name-container {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .scene-name-container :global(.scene-name-field) {
    width: 100%;
  }
  .settings-scroll-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .section-heading {
    margin: 8px 0 0;
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
  }
  .setting-item {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .description-text {
    margin: 0;
    font-size: 0.82rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.4;
  }
  .dimensions-row {
    display: flex;
    gap: 12px;
  }
  .dim-field {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .dim-field :global(.dim-text-field) {
    width: 100%;
  }
  .bgm-controls {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .bgm-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
  }
  .bgm-header-label {
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
  }
  .add-slot-text-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--np-color-primary);
    cursor: pointer;
    padding: 4px 8px;
    border-radius: var(--np-shape-corner-small);
  }
  .add-slot-text-btn:hover {
    background: var(--np-color-surface-container-highest);
  }
  /* Issue #275: 読み取り専用ロール (viewer / denied) ではスロット追加を押せない。 */
  .add-slot-text-btn:disabled {
    color: var(--np-color-on-surface-variant);
    opacity: 0.5;
    cursor: default;
  }
  .add-slot-text-btn:disabled:hover {
    background: none;
  }
  /* Issue #275: 行を持たないスロット (= 追加フロー) のエラー。
     行のエラーと違い行頭のインデントは不要なので、節の左端に揃える。 */
  .bgm-add-error-text {
    margin: 0;
    font-size: 0.8rem;
    line-height: 1.3;
    color: var(--np-color-error);
  }
  .bgm-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .bgm-slot-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  :global(.bgm-pending-progress) {
    --np-circular-progress-size: 1.25rem;
  }
  .bgm-error-text {
    margin: 0;
    padding-left: 2.5rem;
    font-size: 0.8rem;
    line-height: 1.3;
    color: var(--np-color-error);
  }
  .scene-state-error {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    line-height: 1.3;
    color: var(--np-color-error);
  }
  .bg-image-pending {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    line-height: 1.3;
    color: var(--np-color-on-surface-variant);
  }
  /* トランスポート (Issue #250): 接続ボタングループは内容幅に固定し、
     スライダーには残り幅を渡す。各ボタンは正方形に近い固定幅にする。 */
  :global(.bgm-transport-group) {
    flex: 0 0 auto;
  }
  :global(.bgm-transport-group .bgm-transport-btn) {
    flex: 0 0 auto !important;
    height: 2.5rem !important;
  }
  /* 停止/再生ボタン: 選択ボタン（1:1）に対してより細い幅 (narrow) */
  :global(.bgm-play-btn.narrow-btn) {
    width: 2rem !important;
    min-width: 2rem !important;
    max-width: 2rem !important;
    height: 2.5rem !important;
    flex-shrink: 0;
  }
  /* 選択ボタン: 縦横比 1:1 */
  :global(.bgm-select-btn.square-btn) {
    width: 2.5rem !important;
    min-width: 2.5rem !important;
    max-width: 2.5rem !important;
    height: 2.5rem !important;
    aspect-ratio: 1 / 1;
    flex-shrink: 0;
  }
  /* 最初から再生ボタン (⏪) */
  :global(.bgm-restart-btn) {
    width: 2rem !important;
    min-width: 2rem !important;
    max-width: 2rem !important;
    height: 2.5rem !important;
    flex-shrink: 0;
  }
  .slider-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .slider-wrapper :global(.np-slider) {
    flex: 1;
  }
  .vol-label {
    font-size: 0.8rem;
    width: 36px;
    text-align: right;
    color: var(--np-color-on-surface-variant);
    flex-shrink: 0;
  }
  /* 削除ボタン (Issue #226): 行の右端に固定し、スライダーに押し潰されないようにする */
  :global(.bgm-delete-btn) {
    flex-shrink: 0;
  }
  .management-buttons {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  /* 管理操作の送信中インジケーター (Issue #109 / #143): 波線の CircularProgress */
  :global(.scene-action-progress) {
    --np-circular-progress-size: 1.25rem;
  }
  /* 管理操作の失敗表示: 管理コントロール群の下に中央寄せ (Issue #109 / #143) */
  .scene-action-error {
    margin: 0;
    padding: 0 8px;
    text-align: center;
    font-size: 0.85rem;
    line-height: 1.3;
    color: var(--np-color-error);
  }
  .scene-actions-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    width: 100%;
    overflow: hidden;
  }
  .action-btn-col {
    display: flex;
    min-width: 0;
    transition: flex var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
                flex-grow var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }
  .action-col-create {
    flex: 5 1 0%;
  }
  .action-col-delete-logs {
    flex: 3.2 1 0%;
  }
  .action-col-delete-scene {
    flex: 2.2 1 0%;
  }
  .action-btn-col:hover,
  .action-btn-col.hovered,
  .action-btn-col:focus-within {
    flex: 10 1 0% !important;
  }
  .scene-actions-row.has-hover .action-btn-col:not(.hovered):not(:focus-within),
  .scene-actions-row:hover .action-btn-col:not(:hover):not(:focus-within) {
    flex: 1.5 1 0%;
  }
  :global(.action-square-btn) {
    width: 100%;
    min-width: 0 !important;
    --_button-min-width: 0;
    --np-button-padding: 0.5rem;
    padding-inline: 0.5rem !important;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: clip;
    transition: all var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }
  :global(.action-square-btn .children-wrapper) {
    min-width: 0;
    overflow: hidden;
    display: inline-flex;
    align-items: center;
  }
  .action-btn-label {
    display: inline-block;
    max-width: 0;
    opacity: 0;
    overflow: hidden;
    white-space: nowrap;
    margin-left: 0;
    font-size: 0.95rem;
    font-weight: 600;
    vertical-align: middle;
    transition: max-width var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
                opacity var(--np-motion-expressive-fast-effects, 150ms ease),
                margin-left var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }
  .action-btn-col:hover .action-btn-label,
  .action-btn-col.hovered .action-btn-label,
  .action-btn-col:focus-within .action-btn-label {
    max-width: 180px;
    opacity: 1;
    margin-left: 6px;
  }
  :global(.delete-scene-tonal-btn),
  :global(.destructive-tonal-btn) {
    --np-tonal-button-container-color: var(--np-color-error-container, #ffdad6);
    --np-tonal-button-label-text-color: var(--np-color-on-error-container, #410002);
    --np-button-container-color: var(--np-color-error-container, #ffdad6);
    --np-button-label-text-color: var(--np-color-on-error-container, #410002);
  }
  .fullscreen-dialog-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .dialog-box {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    border-radius: var(--np-shape-corner-extra-large);
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .dialog-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
  }
  .dialog-input-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dialog-input-row :global(.new-scene-name-field) {
    width: 100%;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .dialog-footer-right {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .warning-text {
    color: var(--np-color-tertiary);
  }
  .warning-body {
    margin: 0;
    font-size: 0.92rem;
    color: var(--np-color-on-surface-variant);
    text-align: center;
  }
</style>
