<script lang="ts">
  /**
   * Main App component for @alhalte/argenterie.
   *
   * Layout adheres to Material 3 Expressive:
   * - Landscape: Left side field panel, Right side Side Sheet control panel (`Sheet` with `placement="end"`).
   * - Portrait: Full-screen field panel with Bottom Sheet control panel (`Sheet` with `placement="bottom"`).
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie`
   * - GitHub Issue #32: Side sheets (landscape) & Bottom sheets (portrait)
   */
  import 'noph-ui/defaultTheme';
  import { Button, IconButton, Sheet } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { BgmSlot, Character, ExtensionItem, LogRecord, Scene, ViewTransform, VisitedRoom } from '../types/index.js';
  import { saveRecentRoom, type RecentRoom } from '../utils/recentRooms.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../utils/assets.js';
  import { fetchRoomInfo, resolveRoomFromLocation, RoomApiError } from '../utils/home.js';
  import { ChunkLogManager } from '../utils/log.js';
  import type { SceneMovementEvent } from '../utils/sceneGraph.js';
  import {
    createDefaultJuices,
    downloadSceneBackground,
    getNormalizedBgmSlots,
    inheritJuices,
    moveMemberToScene,
    parseSceneBackgroundState,
    SCENE_BACKGROUND_HEIGHT_STATE_KEY,
    SCENE_BACKGROUND_STATE_KEY,
    SCENE_BACKGROUND_WIDTH_STATE_KEY,
    SCENE_NAME_STATE_KEY,
  } from '../utils/sceneSettings.js';
  import { applyThemeToDocument } from '../utils/theme.js';
  import {
    DEFAULT_OFFICIAL_EXTENSIONS,
    ExtensionPipeline,
    loadDefaultExtensions,
    WasmExtensionInstance,
  } from '../wasm/runner.js';
  import AssetPickerModal from './Control/AssetPickerModal.svelte';
  import CharacterEditModal from './Control/CharacterEditModal.svelte';
  import ControlPanel from './Control/ControlPanel.svelte';
  import FieldPanel from './Field/FieldPanel.svelte';
  import GateScreen from './Gate/GateScreen.svelte';
  import HomeScreen from './Home/HomeScreen.svelte';
  import OfflineIndicator from './OfflineIndicator.svelte';
  import RoomPermissionModal from './Control/RoomPermissionModal.svelte';
  import {
    connectionManager as defaultConnectionManager,
    ConnectionManager,
    type ConnectionState,
  } from '../utils/connection.js';
  import {
    RoomSocket,
    OperationTimeoutError,
    ServerOperationError,
    parseCharacter,
    sceneListEntrySummary,
    tinToJson,
    type SceneSummary,
  } from '../utils/roomSocket.js';
  import { getOutbox, type OutboxEntry } from '../utils/outbox.js';
  import { resolveRoomKey } from '../utils/cellarTransfer.js';
  import {
    getRoomPermissions,
    getSavedAccountId,
    getSavedSessionId,
    type RoomPermissionItem,
    type RoomRole,
  } from '../utils/session.js';

  let {
    initialRoom,
    initialScenes = [],
    initialCharacters = [],
    initialExtensions = [],
    initialMovements = [],
    isAdmin: isAdminOverride,
    userRole: initialUserRole = null,
    isViewer: explicitIsViewer,
    isForbidden: initialIsForbidden = false,
    initialPermissions = [],
    connectionManager: customConnectionManager,
    connectionState,
    onReconnect,
    debugMode,
    debugAlwaysShow = false,
    roomSocket: customRoomSocket,
    roomKey,
    fetchFn,
  }: {
    initialRoom?: VisitedRoom | null;
    initialScenes?: Scene[];
    initialCharacters?: Character[];
    initialExtensions?: ExtensionItem[];
    initialMovements?: SceneMovementEvent[];
    /**
     * Explicit test / preview override for the GM-only UI. When omitted, the
     * server role (`userRole === 'gm'`) is authoritative (Issue #156).
     */
    isAdmin?: boolean;
    userRole?: RoomRole | null;
    isViewer?: boolean;
    isForbidden?: boolean;
    initialPermissions?: RoomPermissionItem[];
    connectionManager?: ConnectionManager;
    connectionState?: ConnectionState;
    onReconnect?: () => Promise<void> | void;
    debugMode?: boolean;
    debugAlwaysShow?: boolean;
    roomSocket?: RoomSocket;
    /**
     * 32-byte room key used to encrypt / decrypt extension wasm in the cellar
     * (Issue #109 / sub-issue #145). Defaults to `currentRoom.url` and then the
     * URL fragment; tests and embedders can inject one.
     */
    roomKey?: Uint8Array | null;
    /** Injectable fetch implementation for the room preflight (tests). */
    fetchFn?: typeof fetch;
  } = $props();

  let activeConnectionManager = $derived(customConnectionManager ?? defaultConnectionManager);
  let currentConnectionState = $state<ConnectionState>(
    connectionState ?? defaultConnectionManager.getState(),
  );

  $effect(() => {
    if (connectionState !== undefined) {
      activeConnectionManager.setState(connectionState);
    }
  });

  $effect(() => {
    if (onReconnect) {
      activeConnectionManager.setReconnectHandler(onReconnect);
    }
  });

  $effect(() => {
    const unsubscribe = activeConnectionManager.subscribe((st) => {
      currentConnectionState = connectionState !== undefined ? connectionState : st;
    });
    return unsubscribe;
  });

  async function handleReconnectFromIndicator() {
    // A failed join preflight blocks the socket; retry the HTTP check first.
    if (initialRoom === undefined && activeRoomId && preflightStatus !== 'ok') {
      handleRetryPreflight();
      return;
    }
    await activeConnectionManager.reconnect();
  }

  let currentRoom = $state<VisitedRoom | null>(
    initialRoom !== undefined ? initialRoom : resolveRoomFromLocation(),
  );

  // User role & permission states (Issue #94 / #156)
  let userRole = $state<RoomRole | null>(initialUserRole);
  let isViewer = $derived(explicitIsViewer !== undefined ? explicitIsViewer : userRole === 'viewer');
  /**
   * GM-only UI. The server role is authoritative; `isAdmin` is only an explicit
   * test / preview override, and a viewer can never be an admin (Issue #156).
   */
  let isAdmin = $derived(
    !isViewer && (isAdminOverride !== undefined ? isAdminOverride : userRole === 'gm'),
  );
  let isForbidden = $state(initialIsForbidden);
  let isPermissionModalOpen = $state(false);
  let roomPermissions = $state<RoomPermissionItem[]>([...initialPermissions]);

  /**
   * Join preflight state (Issue #156).
   *
   * `GET /api/rooms/{room_id}` runs before any WebSocket is opened so 403 / 404
   * are known up front. `initialRoom` is an explicit test / preview override:
   * it skips the preflight (and keeps the role supplied by props).
   */
  type RoomPreflightStatus =
    | 'idle'
    | 'checking'
    | 'ok'
    | 'forbidden'
    | 'not-found'
    | 'error';
  let preflightStatus = $state<RoomPreflightStatus>(initialRoom !== undefined ? 'ok' : 'idle');
  let preflightError = $state<string | null>(null);

  /** Room id that the preflight / socket effects follow. */
  let activeRoomId = $derived(
    currentRoom?.id || (currentRoom as any)?.roomId || null,
  );

  /** Same 2-second cadence as `RoomSocket`; failures are bounded, not infinite. */
  const PREFLIGHT_RETRY_DELAY_MS = 2000;
  const MAX_PREFLIGHT_RETRIES = 5;
  let preflightAttempts = 0;
  let preflightToken = 0;
  let preflightTimer: ReturnType<typeof setTimeout> | null = null;

  function clearPreflightTimer(): void {
    if (preflightTimer !== null) {
      clearTimeout(preflightTimer);
      preflightTimer = null;
    }
  }

  /** Stops an in-flight preflight (room change / unmount). */
  function cancelPreflight(): void {
    preflightToken += 1;
    preflightAttempts = 0;
    clearPreflightTimer();
  }

  /** Runs the join preflight for `roomId`, resetting any previous retry loop. */
  async function beginPreflight(roomId: string): Promise<void> {
    cancelPreflight();
    const token = preflightToken;
    preflightStatus = 'checking';
    preflightError = null;
    await attemptPreflight(roomId, token);
  }

  async function attemptPreflight(roomId: string, token: number): Promise<void> {
    try {
      const info = await fetchRoomInfo(roomId, fetchFn ? { fetchFn } : {});
      if (token !== preflightToken) return;

      if (currentRoom) {
        // Mutate in place: replacing the object would retrigger the preflight
        // effect through `activeRoomId` even though the id did not change.
        currentRoom.name = info.name;
      }
      userRole = info.role;
      isForbidden = false;
      preflightStatus = 'ok';
      preflightAttempts = 0;
      preflightError = null;
      activeConnectionManager.setState('connected');
    } catch (error) {
      if (token !== preflightToken) return;

      if (error instanceof RoomApiError) {
        if (error.kind === 'unauthorized') {
          // Sign-in is the only useful next step; the Gate screen offers it.
          preflightStatus = 'forbidden';
          isForbidden = true;
          activeConnectionManager.setState('disconnected');
          return;
        }
        if (error.kind === 'forbidden') {
          preflightStatus = 'forbidden';
          isForbidden = true;
          activeConnectionManager.setState('disconnected');
          return;
        }
        if (error.kind === 'not-found') {
          preflightStatus = 'not-found';
          activeConnectionManager.setState('disconnected');
          return;
        }
        if (error.kind !== 'network' && (error.status ?? 0) < 500) {
          // A protocol failure will not fix itself; show the retry UI once.
          preflightStatus = 'error';
          preflightError = error.message;
          activeConnectionManager.setState('disconnected');
          return;
        }
      }

      preflightAttempts += 1;
      if (preflightAttempts > MAX_PREFLIGHT_RETRIES) {
        preflightStatus = 'error';
        preflightError =
          'サーバーと通信できませんでした。接続が回復したら再試行してください。';
        activeConnectionManager.setState('disconnected');
        return;
      }

      preflightStatus = 'checking';
      preflightError = null;
      activeConnectionManager.setState('reconnecting');
      preflightTimer = setTimeout(() => {
        preflightTimer = null;
        void attemptPreflight(roomId, token);
      }, PREFLIGHT_RETRY_DELAY_MS);
    }
  }

  /** Manual retry used by the preflight error screen / offline indicator. */
  function handleRetryPreflight(): void {
    if (!activeRoomId) return;
    void beginPreflight(String(activeRoomId));
  }

  /** Leaves the room / gate / error state and returns to the Home screen. */
  function returnHome(): void {
    cancelPreflight();
    isForbidden = false;
    preflightStatus = 'idle';
    preflightError = null;
    userRole = null;
    currentRoom = null;
    if (typeof window !== 'undefined' && window.history?.pushState) {
      window.history.pushState(null, '', '/');
    }
  }

  let isLandscape = $state(
    typeof window !== 'undefined' ? window.innerWidth >= window.innerHeight : true,
  );
  let isControlSheetOpen = $state(true);
  let isControlSheetExpanded = $state(false);
  let sideSheetWidth = $state(440);
  let isResizing = $state(false);

  // Default scenes if none provided
  let scenes = $state<Scene[]>(
    initialScenes.length > 0
      ? initialScenes
      : [
          {
            id: '/',
            name: 'メインシーン',
            juices: createDefaultJuices(),
            viewTransform: { x: 0, y: 0, scale: 1 },
            members: [],
          },
        ],
  );

  let currentSceneId = $state('/');
  let currentScene = $derived.by(() => {
    return scenes.find((s) => s.id === currentSceneId) || scenes[0];
  });

  let characters = $state<Character[]>(initialCharacters);
  let extensions = $state<ExtensionItem[]>(
    initialExtensions.length > 0
      ? initialExtensions
      : DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => ({ ...ext })),
  );
  let movements = $state<SceneMovementEvent[]>(initialMovements);
  let selectedCharacterId = $state<string | undefined>(characters[0]?.id);

  /**
   * Room key used for cellar transfers (Issue #109 / #145 / #156).
   *
   * Resolution order: explicit prop, `currentRoom.url` fragment, then the
   * browser URL fragment. Never leaves the browser.
   */
  let extensionRoomKey = $derived(
    resolveRoomKey({
      roomKey,
      roomUrl: currentRoom?.url || (currentRoom as any)?.roomUrl,
    }),
  );
  /**
   * True once the extension tab handed back an authoritative list. It stops the
   * asynchronous `loadDefaultExtensions()` fallback from overwriting metadata
   * that was already merged from the server.
   */
  let extensionStateApplied = false;

  let accessibleSceneIds = $derived.by(() => {
    return new Set(scenes.map((s) => s.id));
  });

  // Full-screen overlay & dialog state at root viewport level
  type ActiveOverlay =
    | {
        type: 'delete-scene';
        sceneId: string;
        sceneName: string;
      }
    | {
        type: 'delete-scene-logs';
        sceneId: string;
        sceneName: string;
      }
    | {
        type: 'delete-character';
        characterId: string;
        characterName: string;
      }
    | {
        type: 'asset-picker';
        mediaType: AssetMediaType;
        initialScope?: AssetScope;
        onSelect: (dataUrl?: string, asset?: AssetItem) => void;
      }
    | null;

  let activeOverlay = $state<ActiveOverlay>(null);

  function handleRequestDeleteScene(scene: { id: string; name: string }) {
    activeOverlay = {
      type: 'delete-scene',
      sceneId: scene.id,
      sceneName: scene.name,
    };
  }

  function handleRequestDeleteSceneLogs(scene: { id: string; name: string }) {
    activeOverlay = {
      type: 'delete-scene-logs',
      sceneId: scene.id,
      sceneName: scene.name,
    };
  }

  function handleRequestDeleteCharacter(char: { id: string; name: string }) {
    activeOverlay = {
      type: 'delete-character',
      characterId: char.id,
      characterName: char.name,
    };
  }

  function handleRequestAssetPicker(options: {
    mediaType: AssetMediaType;
    initialScope?: AssetScope;
    onSelect: (dataUrl?: string, asset?: AssetItem) => void;
  }) {
    activeOverlay = {
      type: 'asset-picker',
      mediaType: options.mediaType,
      initialScope: options.initialScope,
      onSelect: options.onSelect,
    };
  }

  let activeCharacterEdit = $state<Character | null>(null);

  /** Opens the hoisted editor on an existing character. */
  function handleRequestEditCharacter(char: Character) {
    activeCharacterEdit = char;
    characterEditIsNew = false;
    characterSaveError = null;
    characterSaving = false;
  }

  /**
   * Opens the hoisted editor on a brand-new draft (Issue #109 / #144).
   *
   * Nothing is created or sent yet: the character enters the model only after
   * the server allocates an id (or after the outbox flush does).
   */
  function handleRequestCreateCharacter(draft: Character) {
    if (isViewer) return;
    activeCharacterEdit = draft;
    characterEditIsNew = true;
    characterSaveError = null;
    characterSaving = false;
  }

  /** Closes the root editor and clears its transient save state. */
  function closeCharacterEditPanel() {
    activeCharacterEdit = null;
    characterEditIsNew = false;
    characterSaveError = null;
    characterSaving = false;
  }

  /** `onClose` of the root editor: a save in flight cannot be cancelled. */
  function handleCloseCharacterEdit() {
    if (characterSaving) return;
    closeCharacterEditPanel();
  }

  const chunkManager = new ChunkLogManager();
  const extensionPipeline = new ExtensionPipeline();

  function getRoomLogsStorageKey(roomId: string): string {
    return `alhalte:room-logs:${roomId}`;
  }

  function restoreRoomLogs(roomId: string) {
    if (typeof localStorage === 'undefined') return;
    try {
      const raw = localStorage.getItem(getRoomLogsStorageKey(roomId));
      if (!raw) return;
      const savedRecords = JSON.parse(raw) as LogRecord[];
      if (Array.isArray(savedRecords)) {
        for (const rec of savedRecords) {
          if (rec && typeof rec.id === 'string' && typeof rec.sceneId === 'string') {
            chunkManager.appendRecord(rec);
          }
        }
      }
    } catch {
      // Ignore storage read errors
    }
  }

  function persistRoomLogs(roomId: string) {
    if (typeof localStorage === 'undefined') return;
    try {
      const allRecords: LogRecord[] = [];
      for (const sc of scenes) {
        allRecords.push(...chunkManager.getLoadedRecords(sc.id));
      }
      localStorage.setItem(getRoomLogsStorageKey(roomId), JSON.stringify(allRecords));
    } catch {
      // Ignore storage write errors
    }
  }

  // Orientation listener
  function updateOrientation() {
    if (typeof window !== 'undefined') {
      isLandscape = window.innerWidth >= window.innerHeight;
      const minWidth = window.innerWidth * 0.1;
      if (sideSheetWidth < minWidth) {
        sideSheetWidth = minWidth;
      }
    }
  }

  function handleSideResizeDown(e: PointerEvent) {
    if (e.button !== 0) return;
    isResizing = true;
    const startX = e.clientX;
    const startWidth = sideSheetWidth;

    const onPointerMove = (ev: PointerEvent) => {
      if (!isResizing) return;
      const deltaX = startX - ev.clientX;
      const minWidth = typeof window !== 'undefined' ? window.innerWidth * 0.1 : 120;
      const maxWidth = typeof window !== 'undefined' ? window.innerWidth * 0.9 : 1000;
      let newWidth = startWidth + deltaX;
      if (newWidth < minWidth) newWidth = minWidth;
      if (newWidth > maxWidth) newWidth = maxWidth;
      sideSheetWidth = newWidth;
    };

    const onPointerUp = (ev: PointerEvent) => {
      isResizing = false;
      if (typeof window !== 'undefined') {
        window.removeEventListener('pointermove', onPointerMove);
        window.removeEventListener('pointerup', onPointerUp);
        window.removeEventListener('pointercancel', onPointerUp);
      }
      try {
        (e.currentTarget as HTMLElement)?.releasePointerCapture(e.pointerId);
      } catch {
        // ignore
      }
    };

    try {
      (e.currentTarget as HTMLElement)?.setPointerCapture(e.pointerId);
    } catch {
      // ignore
    }

    if (typeof window !== 'undefined') {
      window.addEventListener('pointermove', onPointerMove);
      window.addEventListener('pointerup', onPointerUp);
      window.addEventListener('pointercancel', onPointerUp);
    }
  }

  function syncRoomFromLocation() {
    if (initialRoom !== undefined) return;
    currentRoom = resolveRoomFromLocation();
  }

  $effect(() => {
    applyThemeToDocument();
    updateOrientation();
    if (initialExtensions.length === 0) {
      loadDefaultExtensions().then((loaded) => {
        if (loaded.length === 0) return;
        if (!extensionStateApplied) {
          extensions = loaded;
          return;
        }
        // The tab already merged the room state. Keep that authoritative list
        // and only fill in wasm bodies it does not have yet (the manifest is the
        // only source of the official `.wasm` files).
        const loadedById = new Map(loaded.map((ext) => [ext.id, ext]));
        extensions = extensions.map((ext) => {
          const official = loadedById.get(ext.id);
          if (!ext.wasmBytes && official?.wasmBytes) {
            return { ...ext, wasmBytes: official.wasmBytes };
          }
          return ext;
        });
      });
    }
    if (typeof window !== 'undefined') {
      window.addEventListener('resize', updateOrientation);
      window.addEventListener('popstate', syncRoomFromLocation);
      window.addEventListener('hashchange', syncRoomFromLocation);
      return () => {
        window.removeEventListener('resize', updateOrientation);
        window.removeEventListener('popstate', syncRoomFromLocation);
        window.removeEventListener('hashchange', syncRoomFromLocation);
      };
    }
  });

  $effect(() => {
    if (currentRoom?.id) {
      restoreRoomLogs(currentRoom.id);
      saveRecentRoom({
        roomId: currentRoom.id || (currentRoom as any).roomId,
        roomName: currentRoom.name || (currentRoom as any).roomName || '名称未設定',
        url: currentRoom.url || (currentRoom as any).roomUrl,
        roomUrl: currentRoom.url || (currentRoom as any).roomUrl,
        visitedAt: (currentRoom as any).visitedAt || currentRoom.lastVisited || Date.now(),
        thumbnailUrl: (currentRoom as any).thumbnailUrl,
        ownerAccountId: (currentRoom as any).ownerAccountId,
        isAdmin: currentRoom.isAdmin,
      });
    }
  });

  $effect(() => {
    const rId = activeRoomId;
    if (rId && typeof fetch !== 'undefined') {
      // The permission list only feeds `RoomPermissionModal`; role and entry
      // authority come from the `GET /api/rooms/{id}` preflight (Issue #156).
      getRoomPermissions(rId)
        .then((perms) => {
          roomPermissions = perms;
        })
        .catch(() => {
          // Keep the cached list; the modal reports its own errors.
        });
    }
  });

  let roomSocket = $state<RoomSocket | null>(customRoomSocket ?? null);

  // ---------------------------------------------------------------------------
  // Message sending & offline outbox (Issue #109 / sub-issue #139)
  // ---------------------------------------------------------------------------
  /** True while a sent message is waiting for the server's `LogEventAppended` echo. */
  let isSending = $state(false);
  /** Number of messages waiting in the durable outbox for the active room. */
  let pendingMessageCount = $state(0);
  /** Guards against overlapping outbox flushes (reconnect can fire repeatedly). */
  let isFlushingOutbox = false;

  /**
   * Room id used to key the durable outbox, or `null` outside a room.
   */
  function getActiveRoomId(): string | null {
    const roomId = currentRoom?.id || (currentRoom as any)?.roomId;
    return typeof roomId === 'string' && roomId.length > 0 ? roomId : null;
  }

  /**
   * Re-reads the outbox and republishes the queue notice count.
   *
   * Called after every outbox mutation (enqueue / flush / room switch) so the
   * `OfflineQueueNotice` count always reflects IndexedDB rather than an
   * optimistic in-memory counter.
   */
  async function refreshPendingMessageCount() {
    const roomId = getActiveRoomId();
    if (!roomId) {
      pendingMessageCount = 0;
      return;
    }
    try {
      const pending = await getOutbox().list(roomId, 'message');
      pendingMessageCount = pending.length;
    } catch (err) {
      console.warn('Failed to read the offline outbox:', err);
    }
  }

  /**
   * Persists a message that could not be delivered into the durable outbox
   * (30-day TTL, see `outbox.ts`) and refreshes the notice count.
   *
   * The payload keeps the `LogRecord` (including its `id`) so the flushed copy
   * and a late echo of the same record dedupe on `LogRecord.id` inside
   * `ChunkLogManager` instead of rendering twice.
   */
  async function enqueueOfflineMessage(roomId: string, sceneId: string, record: LogRecord) {
    try {
      await getOutbox().enqueue({
        roomId,
        kind: 'message',
        payload: { sceneId, record },
      });
    } catch (err) {
      // Last resort: keep the message visible locally rather than losing it.
      console.error('Failed to queue the offline message:', err);
      chunkManager.appendRecord(record);
    }
    await refreshPendingMessageCount();
  }

  /**
   * Replays the room's queued messages after a reconnect.
   *
   * Each entry is handed to `requestAppendLog`, which resolves only once the
   * matching echo arrives. `RoomSocket` already appends that echo into
   * `chunkManager`, so nothing is appended here; a rejection leaves the entry
   * queued for the next reconnect. Flushing is skipped entirely while the socket
   * is not connected.
   */
  async function flushOutbox() {
    if (isFlushingOutbox) return;
    const roomId = getActiveRoomId();
    if (!roomId) return;
    if (!roomSocket || !roomSocket.isConnected) return;

    isFlushingOutbox = true;
    try {
      await getOutbox().flush(roomId, async (entry) => {
        if (entry.kind === 'character') {
          // Character CRUD, queued while offline or after a save timeout
          // (Issue #109 / sub-issue #144).
          return flushCharacterOutboxEntry(entry);
        }
        if (entry.kind !== 'message') {
          // Other kinds are owned by the features that queued them.
          return false;
        }
        const payload = entry.payload as
          | { sceneId?: unknown; record?: LogRecord }
          | null
          | undefined;
        const record = payload?.record;
        if (!record || typeof record !== 'object') {
          console.warn('Dropping a malformed outbox message entry:', entry.id);
          return true;
        }
        const sceneId =
          typeof payload?.sceneId === 'string' ? payload.sceneId : currentScene.id;
        if (!roomSocket || !roomSocket.isConnected) {
          throw new Error('room socket is not connected');
        }
        await roomSocket.requestAppendLog(sceneId, record);
        return true;
      });
    } catch (err) {
      console.warn('Failed to flush the offline outbox:', err);
    } finally {
      isFlushingOutbox = false;
      await refreshPendingMessageCount();
      await refreshPendingCharacterCount();
    }
  }

  // Flush on mount (a previous session may have left messages queued) and on
  // every transition into `connected`.
  let lastConnectionState: ConnectionState | null = null;
  $effect(() => {
    const state = currentConnectionState;
    const previous = lastConnectionState;
    lastConnectionState = state;
    if (state !== 'connected' || previous === 'connected') return;
    void flushOutbox();
  });

  // Keep the queue notice count in sync with the active room.
  $effect(() => {
    const roomId = currentRoom?.id || (currentRoom as any)?.roomId;
    if (!roomId) {
      pendingMessageCount = 0;
      return;
    }
    void refreshPendingMessageCount();
  });

  // ---------------------------------------------------------------------------
  // Character synchronization (Issue #109 / sub-issue #144)
  // ---------------------------------------------------------------------------
  /** Number of character operations waiting in the durable outbox. */
  let pendingCharacterCount = $state(0);
  /** True from the save request until its acknowledgement (or queue fallback). */
  let characterSaving = $state(false);
  /** Non-retryable save failure rendered inside the still-open edit panel. */
  let characterSaveError = $state<string | null>(null);
  /** True while the root edit panel edits a draft that was never sent. */
  let characterEditIsNew = $state(false);

  /** Payload persisted in a `kind: 'character'` outbox entry. */
  type QueuedCharacterPayload = {
    operation: 'create' | 'update' | 'delete';
    character?: Character;
    characterId?: string;
  };

  const CHARACTER_SAVE_FAILED_MESSAGE = '保存できませんでした。時間をおいて再試行してください。';
  const CHARACTER_PERMISSION_DENIED_MESSAGE = 'キャラクターを編集する権限がありません。';

  /** True for a server rejection that retrying will not fix. */
  function isPermissionDeniedError(err: unknown): boolean {
    return err instanceof ServerOperationError && err.code === 'PermissionDenied';
  }

  /**
   * True when the operation is worth replaying later: a timeout, a socket that
   * dropped before the ack, or any other server error except permission denial.
   * Mirrors the send-path classification in {@link handleSendMessage}.
   */
  function isRetryableCharacterError(err: unknown): boolean {
    return (
      err instanceof OperationTimeoutError ||
      (err instanceof ServerOperationError && !isPermissionDeniedError(err))
    );
  }

  /** Reads the server-allocated id from a `CharacterCreated` / `CharacterUpdated` ack. */
  function serverCharacterId(
    ack: { character_id?: unknown } | null | undefined,
    fallback: string,
  ): string {
    const id = ack?.character_id;
    return typeof id === 'number' || typeof id === 'string' ? String(id) : fallback;
  }

  /**
   * Re-reads how many `kind: 'character'` entries the active room has queued and
   * republishes the count for the tab's `OfflineQueueNotice`.
   */
  async function refreshPendingCharacterCount(): Promise<void> {
    const roomId = getActiveRoomId();
    if (!roomId) {
      pendingCharacterCount = 0;
      return;
    }
    try {
      const pending = await getOutbox().list(roomId, 'character');
      pendingCharacterCount = pending.length;
    } catch (err) {
      console.warn('Failed to read the character outbox:', err);
    }
  }

  /**
   * Persists a character operation that could not be delivered into the durable
   * outbox (30-day TTL, see `outbox.ts`) and refreshes the queue notice count.
   *
   * Returns `false` when persistence itself failed; the caller then keeps the
   * edit panel open instead of silently losing the edit.
   */
  async function queueCharacterOperation(
    roomId: string,
    payload: QueuedCharacterPayload,
  ): Promise<boolean> {
    try {
      await getOutbox().enqueue({ roomId, kind: 'character', payload });
    } catch (err) {
      console.error('Failed to queue the character operation:', err);
      return false;
    }
    await refreshPendingCharacterCount();
    return true;
  }

  /** Replaces the local list with the authoritative `CharacterList`. */
  function applyCharacterList(list: Character[]): void {
    // Replace even when the list is empty: the server list is authoritative and
    // a room may legitimately end up with zero characters.
    characters = list;
    // Keep the selected character valid (or fall back to the first one).
    if (!selectedCharacterId || !characters.some((c) => c.id === selectedCharacterId)) {
      selectedCharacterId = characters[0]?.id;
    }
  }

  /** Applies an acknowledged creation, replacing the draft id with the server id. */
  function applyCreatedCharacter(draft: Character, serverId: string): void {
    const created: Character = { ...draft, id: serverId };
    const index = characters.findIndex((c) => c.id === serverId || c.id === draft.id);
    if (index >= 0) {
      characters = characters.map((c, i) => (i === index ? created : c));
    } else {
      characters = [...characters, created];
    }
    if (!selectedCharacterId || selectedCharacterId === draft.id) {
      selectedCharacterId = serverId;
    }
    // Legacy `handleCreateCharacter` also joined the current scene; keep parity
    // now that the membership change happens only after the ack.
    if (!currentScene.members.includes(serverId)) {
      currentScene.members.push(serverId);
    }
  }

  /** Applies an acknowledged update, replacing the record by id. */
  function applyUpdatedCharacter(character: Character, serverId: string = character.id): void {
    const updated: Character = { ...character, id: serverId };
    const index = characters.findIndex((c) => c.id === character.id || c.id === serverId);
    if (index >= 0) {
      characters = characters.map((c, i) => (i === index ? updated : c));
    } else {
      characters = [...characters, updated];
    }
    if (selectedCharacterId === character.id) selectedCharacterId = serverId;
  }

  /** Removes a character and its scene memberships from the local model. */
  function removeCharacterLocally(characterId: string): void {
    characters = characters.filter((c) => c.id !== characterId);
    for (const sc of scenes) {
      sc.members = sc.members.filter((id) => id !== characterId);
    }
    if (selectedCharacterId === characterId) {
      selectedCharacterId = characters[0]?.id;
    }
  }

  /**
   * Replays one queued `kind: 'character'` entry from the outbox.
   *
   * Returns `true` only after the server acknowledgement, which lets
   * `Outbox.flush` delete the entry. A retryable failure returns `false` (the
   * entry stays queued for the next reconnect); a non-retryable failure throws,
   * which also leaves it queued — mirroring the message flush contract.
   */
  async function flushCharacterOutboxEntry(entry: OutboxEntry): Promise<boolean> {
    const payload = entry.payload as QueuedCharacterPayload | null | undefined;
    const operation = payload?.operation;
    const socket = roomSocket;
    if (!socket || !socket.isConnected) {
      // No ack possible: keep the entry queued.
      throw new Error('room socket is not connected');
    }

    try {
      if (operation === 'create') {
        const character = payload?.character;
        if (!character || typeof character !== 'object') {
          console.warn('Dropping a malformed outbox character entry:', entry.id);
          return true;
        }
        const ack = await socket.requestCreateCharacter(character);
        applyCreatedCharacter(character, serverCharacterId(ack, character.id));
        return true;
      }
      if (operation === 'update') {
        const character = payload?.character;
        if (!character || typeof character !== 'object') {
          console.warn('Dropping a malformed outbox character entry:', entry.id);
          return true;
        }
        const ack = await socket.requestUpdateCharacter(Number(character.id), character);
        applyUpdatedCharacter(character, serverCharacterId(ack, character.id));
        return true;
      }
      if (operation === 'delete') {
        const characterId = payload?.characterId;
        if (typeof characterId !== 'string' || characterId.length === 0) {
          console.warn('Dropping a malformed outbox character entry:', entry.id);
          return true;
        }
        await socket.requestDeleteCharacter(Number(characterId));
        removeCharacterLocally(characterId);
        return true;
      }
      // Unknown operation: nothing can replay it, so drop it instead of
      // blocking every later entry forever.
      console.warn('Dropping an unknown outbox character entry:', entry.id, operation);
      return true;
    } catch (err) {
      if (isRetryableCharacterError(err)) return false;
      throw err;
    }
  }

  // Keep the character queue notice count in sync with the active room.
  $effect(() => {
    const roomId = currentRoom?.id || (currentRoom as any)?.roomId;
    if (!roomId) {
      pendingCharacterCount = 0;
      return;
    }
    void refreshPendingCharacterCount();
  });

  // Persist the room log cache from a `chunkManager` subscription: the
  // `LogEventAppended` echo is appended to the manager before its subscribers
  // run, so this never races the echo or double-adds a record.
  $effect(() => {
    let persistScheduled = false;
    return chunkManager.subscribe(() => {
      if (persistScheduled) return;
      persistScheduled = true;
      queueMicrotask(() => {
        persistScheduled = false;
        const roomId = getActiveRoomId();
        if (roomId) persistRoomLogs(roomId);
      });
    });
  });

  // Join preflight: resolve the room on the server before opening the socket
  // (Issue #156). `initialRoom` is an explicit test / preview override and skips
  // the check so embedders can supply a role directly.
  $effect(() => {
    if (initialRoom !== undefined) return;

    const roomId = activeRoomId;
    if (!roomId) {
      cancelPreflight();
      preflightStatus = 'idle';
      return;
    }

    void beginPreflight(String(roomId));
    return () => {
      cancelPreflight();
    };
  });

  $effect(() => {
    if (customRoomSocket) {
      roomSocket = customRoomSocket;
      return;
    }

    // Never open the WebSocket until the preflight confirmed the room.
    if (preflightStatus !== 'ok') {
      if (roomSocket) {
        roomSocket.disconnect();
        roomSocket = null;
      }
      return;
    }

    const roomId = activeRoomId;
    if (roomId) {
      const socket = new RoomSocket({
        roomId,
        url: currentRoom?.url || (currentRoom as any)?.roomUrl,
        accountId: getSavedAccountId() ?? undefined,
        sessionId: getSavedSessionId() ?? undefined,
        connectionManager: activeConnectionManager,
        chunkLogManager: chunkManager,
        initialSceneId: currentSceneId,
        autoConnect: true,
        onStateSnapshot: (sceneId, states) => {
          applyStateSnapshot(sceneId, states);
        },
        onError: (err) => {
          if (
            err.message?.includes('全体公開されていない部屋です') ||
            err.code === '403' ||
            err.code === 'FORBIDDEN'
          ) {
            isForbidden = true;
          }
        },
        onClose: (event) => {
          // 1008 / 4403 / 4404 mean the server rejected the join. HTTP 403 /
          // 404 are already classified by the preflight; this remains the
          // fallback for a socket that closes after a successful preflight.
          if (event.code === 1008 || event.code === 4403 || event.code === 4404) {
            isForbidden = true;
          }
        },
      });
      roomSocket = socket;

      return () => {
        socket.disconnect();
        if (roomSocket === socket) {
          roomSocket = null;
        }
      };
    } else {
      if (roomSocket && !customRoomSocket) {
        roomSocket.disconnect();
        roomSocket = null;
      }
    }
  });

  // ---------------------------------------------------------------------------
  // Scene-state hydration: BGM / background / name / view / pieces (Issue #159)
  // ---------------------------------------------------------------------------
  /** Scene-state key carrying a slot's track; legacy `std::bgm` is slot 1. */
  const BGM_SLOT_STATE_KEY = /^std::bgm(\d*)$/;

  /** Scene-state key carrying a slot volume (`bgm::<slot>::volume`). */
  const BGM_VOLUME_STATE_KEY = /^bgm::(\d+)::volume$/;

  /** `piece::<characterId>::placed` and `piece::<characterId>::pos` keys. */
  const PIECE_PLACED_STATE_KEY = /^piece::(.+)::placed$/;
  const PIECE_POS_STATE_KEY = /^piece::(.+)::pos$/;

  /**
   * Parses the BGM slot of a `std::bgm<N>` key.
   *
   * `std::bgm` is slot 1 (legacy) and `std::bgm2`, `std::bgm3`, ... are the
   * arbitrary slots the server accepts; `std::bgm1` is accepted as slot 1 too.
   */
  function bgmSlotFromStateKey(key: string): number | null {
    const match = BGM_SLOT_STATE_KEY.exec(key);
    if (!match) return null;
    const raw = match[1];
    const slot = raw === '' ? 1 : Number(raw);
    return Number.isInteger(slot) && slot >= 1 ? slot : null;
  }

  /** True when a `StateUpdated` / `StateDeleted` key belongs to the BGM schema. */
  function isBgmStateKey(key: string): boolean {
    return bgmSlotFromStateKey(key) !== null || BGM_VOLUME_STATE_KEY.test(key);
  }

  /** Mirrors a slot's tin id (and optionally volume) into the legacy `bgm1..3` fields. */
  function syncLegacyBgmFields(
    juices: Scene['juices'],
    slot: number,
    tinId: string | null,
    volume?: number,
  ) {
    const value = tinId ?? undefined;
    if (slot === 1) {
      juices.bgm1 = value;
      if (volume !== undefined) juices.bgm1Volume = volume;
    } else if (slot === 2) {
      juices.bgm2 = value;
      if (volume !== undefined) juices.bgm2Volume = volume;
    } else if (slot === 3) {
      juices.bgm3 = value;
      if (volume !== undefined) juices.bgm3Volume = volume;
    }
  }

  /** Replaces `juices.bgmSlots` with a normalized copy (legacy `bgm1..3` included). */
  function setNormalizedBgmSlots(juices: Scene['juices'], slots: BgmSlot[]) {
    juices.bgmSlots = slots.map((slot) => ({ ...slot }));
  }

  /**
   * Returns `slots` with `slotNum` present. Arbitrary slot numbers are created
   * on demand so a `std::bgm4` snapshot hydrates even when the client has only
   * ever rendered slots 1..3.
   */
  function ensureBgmSlot(slots: BgmSlot[], slotNum: number): BgmSlot[] {
    if (slots.some((slot) => slot.slot === slotNum)) return slots;
    return [...slots, { slot: slotNum, tinId: null, volume: 0.8, isPlaying: false }];
  }

  /**
   * Applies a BGM `StateUpdated` frame.
   *
   * Track values are the 64-hex cellar tin id; the legacy plain-string form of
   * `std::bgm` is still accepted. Tin-encoded values are decoded first because
   * `RoomSocket.subscribe` forwards the raw frame.
   */
  function applyBgmStateUpdated(scene: Scene, key: string, rawValue: unknown) {
    const value = tinToJson(rawValue);

    const volumeMatch = BGM_VOLUME_STATE_KEY.exec(key);
    if (volumeMatch) {
      const slotNum = Number(volumeMatch[1]);
      const volume = typeof value === 'number' ? value : Number(value);
      if (!Number.isFinite(volume)) return;
      const clamped = Math.max(0, Math.min(1, volume));
      const slots = ensureBgmSlot(getNormalizedBgmSlots(scene.juices), slotNum).map((slot) =>
        slot.slot === slotNum ? { ...slot, volume: clamped } : { ...slot },
      );
      setNormalizedBgmSlots(scene.juices, slots);
      const updated = slots.find((slot) => slot.slot === slotNum);
      syncLegacyBgmFields(scene.juices, slotNum, updated?.tinId ?? null, clamped);
      return;
    }

    const slotNum = bgmSlotFromStateKey(key);
    if (slotNum === null) return;
    const tinId = typeof value === 'string' && value.length > 0 ? value : null;
    const slots = ensureBgmSlot(getNormalizedBgmSlots(scene.juices), slotNum).map((slot) =>
      slot.slot === slotNum ? { ...slot, tinId, isPlaying: tinId !== null } : { ...slot },
    );
    setNormalizedBgmSlots(scene.juices, slots);
    syncLegacyBgmFields(scene.juices, slotNum, tinId);
  }

  /** Applies a BGM `StateDeleted` frame (stopping a slot deletes its state key). */
  function applyBgmStateDeleted(scene: Scene, key: string) {
    const volumeMatch = BGM_VOLUME_STATE_KEY.exec(key);
    if (volumeMatch) {
      const slotNum = Number(volumeMatch[1]);
      const slots = ensureBgmSlot(getNormalizedBgmSlots(scene.juices), slotNum).map((slot) =>
        slot.slot === slotNum ? { ...slot, volume: 0.8 } : { ...slot },
      );
      setNormalizedBgmSlots(scene.juices, slots);
      const updated = slots.find((slot) => slot.slot === slotNum);
      syncLegacyBgmFields(scene.juices, slotNum, updated?.tinId ?? null, 0.8);
      return;
    }

    const slotNum = bgmSlotFromStateKey(key);
    if (slotNum === null) return;
    const slots = ensureBgmSlot(getNormalizedBgmSlots(scene.juices), slotNum).map((slot) =>
      slot.slot === slotNum ? { ...slot, tinId: null, isPlaying: false } : { ...slot },
    );
    setNormalizedBgmSlots(scene.juices, slots);
    syncLegacyBgmFields(scene.juices, slotNum, null);
  }

  // ---------------------------------------------------------------------------
  // Background image / dimensions hydration (Issue #159)
  // ---------------------------------------------------------------------------
  /** Last background reference applied per scene, keyed `tinId:mimeType`. */
  const appliedBackgroundKeys = new Map<string, string>();
  /** In-flight background downloads per scene, so a reconnect does not refetch. */
  const backgroundDownloads = new Map<string, string>();

  /**
   * Applies a `std::b-img` value: downloads the referenced tin and stores a data
   * URL in `juices.bgImage`. Data URLs written by an older client are still
   * accepted as-is. The download is skipped when the same reference is already
   * applied, making snapshot re-application (reconnect / scene switch) cheap.
   */
  function applyBackgroundImage(scene: Scene, rawValue: unknown) {
    const decoded = tinToJson(rawValue);
    if (typeof decoded === 'string' && decoded.startsWith('data:')) {
      appliedBackgroundKeys.delete(scene.id);
      scene.juices.bgImage = decoded;
      return;
    }

    const parsed = parseSceneBackgroundState(decoded);
    if (!parsed) {
      appliedBackgroundKeys.delete(scene.id);
      scene.juices.bgImage = undefined;
      return;
    }

    const reference = `${parsed.tinId}:${parsed.mimeType}`;
    if (appliedBackgroundKeys.get(scene.id) === reference && scene.juices.bgImage) return;

    appliedBackgroundKeys.set(scene.id, reference);
    const roomKey = extensionRoomKey;
    if (!roomKey) {
      // No key means the image cannot be decrypted; keep the reference so a
      // later snapshot with a key can retry, and leave the previous preview.
      appliedBackgroundKeys.delete(scene.id);
      return;
    }

    const inFlight = backgroundDownloads.get(scene.id);
    if (inFlight === reference) return;
    backgroundDownloads.set(scene.id, reference);

    void downloadSceneBackground(parsed, roomKey)
      .then((dataUrl) => {
        if (backgroundDownloads.get(scene.id) === reference) {
          backgroundDownloads.delete(scene.id);
        }
        appliedBackgroundKeys.set(scene.id, reference);
        const target = scenes.find((candidate) => candidate.id === scene.id);
        if (target) target.juices.bgImage = dataUrl;
      })
      .catch((error) => {
        console.warn('[App] Failed to load the scene background image:', error);
        if (backgroundDownloads.get(scene.id) === reference) {
          backgroundDownloads.delete(scene.id);
        }
        appliedBackgroundKeys.delete(scene.id);
      });
  }

  function applyBackgroundDimensions(scene: Scene, key: string, rawValue: unknown) {
    const value = tinToJson(rawValue);
    const numeric = typeof value === 'number' ? value : Number(value);
    const parsed = Number.isFinite(numeric) && numeric > 0 ? numeric : undefined;
    if (key === SCENE_BACKGROUND_WIDTH_STATE_KEY) {
      scene.juices.bgWidth = parsed;
    } else {
      scene.juices.bgHeight = parsed;
    }
  }

  // ---------------------------------------------------------------------------
  // Piece hydration (Issue #159)
  // ---------------------------------------------------------------------------
  interface PieceState {
    placed?: boolean;
    pos?: { x: number; y: number };
  }
  /** Piece state per scene, applied to `characters` for the current scene. */
  const pieceStatesByScene = new Map<string, Map<string, PieceState>>();

  function parsePieceStateKey(key: string): { characterId: string; kind: 'placed' | 'pos' } | null {
    const placed = PIECE_PLACED_STATE_KEY.exec(key);
    if (placed) return { characterId: placed[1], kind: 'placed' };
    const pos = PIECE_POS_STATE_KEY.exec(key);
    if (pos) return { characterId: pos[1], kind: 'pos' };
    return null;
  }

  function readPiecePosition(value: unknown): { x: number; y: number } | undefined {
    const decoded = tinToJson(value);
    if (decoded && typeof decoded === 'object') {
      const { x, y } = decoded as { x?: unknown; y?: unknown };
      if (typeof x === 'number' && typeof y === 'number') return { x, y };
    }
    return undefined;
  }

  /** Applies the stored piece state of `sceneId` to the character list. */
  function applyPieceStatesToCharacters(sceneId: string) {
    const states = pieceStatesByScene.get(sceneId);
    if (!states) return;
    characters = characters.map((character) => {
      const state = states.get(character.id);
      if (!state) {
        return character.placedAsPiece === false
          ? character
          : { ...character, placedAsPiece: false };
      }
      return {
        ...character,
        placedAsPiece: state.placed === true,
        ...(state.pos ? { piecePosition: { ...state.pos } } : {}),
      };
    });
  }

  function applyPieceStateEntry(sceneId: string, key: string, rawValue: unknown) {
    const parsed = parsePieceStateKey(key);
    if (!parsed) return;
    let states = pieceStatesByScene.get(sceneId);
    if (!states) {
      states = new Map();
      pieceStatesByScene.set(sceneId, states);
    }
    const current = states.get(parsed.characterId) ?? {};
    if (parsed.kind === 'placed') {
      current.placed = Boolean(tinToJson(rawValue));
    } else {
      const pos = readPiecePosition(rawValue);
      if (pos) current.pos = pos;
    }
    states.set(parsed.characterId, current);
    if (sceneId === currentSceneId) applyPieceStatesToCharacters(sceneId);
  }

  function applyPieceStateDeleted(sceneId: string, key: string) {
    const parsed = parsePieceStateKey(key);
    if (!parsed) return;
    const states = pieceStatesByScene.get(sceneId);
    if (!states) return;
    const current = states.get(parsed.characterId);
    if (!current) return;
    if (parsed.kind === 'placed') current.placed = false;
    else delete current.pos;
    states.set(parsed.characterId, current);
    if (sceneId === currentSceneId) applyPieceStatesToCharacters(sceneId);
  }

  // ---------------------------------------------------------------------------
  // Generic scene-state application (snapshot / StateUpdated / SceneInfo)
  // ---------------------------------------------------------------------------
  /** Applies one decoded scene-state entry to the local scene model. */
  function applySceneStateEntry(scene: Scene, key: string, rawValue: unknown) {
    if (key === SCENE_NAME_STATE_KEY) {
      const name = tinToJson(rawValue);
      if (typeof name === 'string' && name.trim().length > 0) {
        scene.name = name;
      }
      return;
    }
    if (isBgmStateKey(key)) {
      applyBgmStateUpdated(scene, key, rawValue);
      return;
    }
    if (key === SCENE_BACKGROUND_STATE_KEY) {
      applyBackgroundImage(scene, rawValue);
      return;
    }
    if (
      key === SCENE_BACKGROUND_WIDTH_STATE_KEY ||
      key === SCENE_BACKGROUND_HEIGHT_STATE_KEY
    ) {
      applyBackgroundDimensions(scene, key, rawValue);
      return;
    }
    if (key === 'view::transform') {
      const value = tinToJson(rawValue);
      if (value && typeof value === 'object') {
        const { x, y, scale } = value as { x?: unknown; y?: unknown; scale?: unknown };
        if (typeof x === 'number') scene.viewTransform.x = x;
        if (typeof y === 'number') scene.viewTransform.y = y;
        if (typeof scale === 'number' && scale > 0) scene.viewTransform.scale = scale;
      }
      return;
    }
    if (parsePieceStateKey(key)) {
      applyPieceStateEntry(scene.id, key, rawValue);
    }
  }

  /** Applies one decoded scene-state deletion to the local scene model. */
  function applySceneStateDeleted(scene: Scene, key: string) {
    if (key === SCENE_NAME_STATE_KEY) return;
    if (isBgmStateKey(key)) {
      applyBgmStateDeleted(scene, key);
      return;
    }
    if (key === SCENE_BACKGROUND_STATE_KEY) {
      appliedBackgroundKeys.delete(scene.id);
      backgroundDownloads.delete(scene.id);
      scene.juices.bgImage = undefined;
      return;
    }
    if (key === SCENE_BACKGROUND_WIDTH_STATE_KEY) {
      scene.juices.bgWidth = undefined;
      return;
    }
    if (key === SCENE_BACKGROUND_HEIGHT_STATE_KEY) {
      scene.juices.bgHeight = undefined;
      return;
    }
    if (parsePieceStateKey(key)) {
      applyPieceStateDeleted(scene.id, key);
    }
  }

  /** Decodes a raw Tin-encoded state map (as carried by `StateSnapshot`). */
  function tinToJsonMap(raw: Record<string, unknown> | null | undefined): Record<string, unknown> {
    const decoded: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(raw ?? {})) {
      decoded[key] = tinToJson(value);
    }
    return decoded;
  }

  /**
   * Applies a `StateSnapshot` for a scene.
   *
   * The snapshot is authoritative for that scene: every supported key is
   * applied, and piece placement is reset for characters that are not listed.
   * Applying the same snapshot again (reconnect / scene switch) is idempotent.
   */
  function applyStateSnapshot(sceneId: string, states: Record<string, unknown> | null | undefined) {
    let scene = scenes.find((candidate) => candidate.id === sceneId);
    if (!scene) {
      const placeholder = createPlaceholderScene(sceneId);
      scenes = [...scenes, placeholder];
      // Re-read through the reactive array so mutations below hit the proxied
      // scene object.
      scene = scenes.find((candidate) => candidate.id === sceneId) ?? placeholder;
    }

    pieceStatesByScene.set(sceneId, new Map());
    for (const [key, value] of Object.entries(states ?? {})) {
      if (parsePieceStateKey(key)) {
        applyPieceStateEntry(sceneId, key, value);
      } else {
        applySceneStateEntry(scene, key, value);
      }
    }
    if (sceneId === currentSceneId) applyPieceStatesToCharacters(sceneId);
  }

  /**
   * Applies a `SceneInfo` summary (create / rename broadcast).
   *
   * The name is server-authoritative: placeholders created from a `SceneList`
   * id are replaced with the announced name without a reload.
   */
  function applySceneSummary(rawSummary: unknown) {
    const summary: SceneSummary | null = sceneListEntrySummary(rawSummary);
    if (!summary) return;
    const scene = scenes.find((candidate) => candidate.id === summary.id);
    const name =
      typeof summary.name === 'string' && summary.name.trim().length > 0
        ? summary.name
        : undefined;
    if (!scene) {
      const placeholder = createPlaceholderScene(summary.id);
      if (name) placeholder.name = name;
      scenes = [...scenes, placeholder];
      return;
    }
    if (name && scene.name !== name) scene.name = name;
  }

  /**
   * Mirrors a scene model updated by `SceneSettingsTab` (Issue #109 / #142).
   *
   * The tab owns the room-state requests; this handler only replaces the scene
   * by id and normalizes the BGM slots so `scenes` stays the single source of
   * truth for the field and every tab.
   */
  function handleUpdateScene(updatedScene: Scene) {
    if (!updatedScene || typeof updatedScene.id !== 'string') return;
    const index = scenes.findIndex((scene) => scene.id === updatedScene.id);
    if (index < 0) return;

    const juices: Scene['juices'] = {
      ...updatedScene.juices,
      bgmSlots: getNormalizedBgmSlots(updatedScene.juices).map((slot) => ({ ...slot })),
    };
    const nextScenes = [...scenes];
    nextScenes[index] = {
      ...updatedScene,
      juices,
      viewTransform: { ...updatedScene.viewTransform },
      members: [...updatedScene.members],
    };
    scenes = nextScenes;
  }

  // ---------------------------------------------------------------------------
  // Scene management actions (Issue #109 / sub-issue #143)
  // ---------------------------------------------------------------------------
  /** Failure text mirrored into `SceneSettingsTab` under the management controls. */
  const SCENE_ACTION_ERROR_MESSAGE = 'サーバーと通信できませんでした';

  /**
   * Id of the scene action in flight (`create`, `delete-logs`, `delete-scene`
   * or `move:<characterId>`), or `null` when idle.
   *
   * `SceneSettingsTab` swaps the pressed icon for a wavy `CircularProgress`
   * while the id matches, so a single string is enough to drive the spinner of
   * every management control.
   */
  let sceneActionPending = $state<string | null>(null);
  /** Failure message of the last scene action, or `null` while healthy. */
  let sceneActionError = $state<string | null>(null);

  /**
   * Scene ids created by this client after a `SceneCreated` ack that a
   * `SceneList` may not list yet.
   *
   * The server broadcasts `SceneList` when it creates a scene, but a list that
   * was already in flight when the ack arrived can predate the new id. Those
   * scenes are protected from the removal pass until the server lists them.
   */
  const pendingSceneListIds = new Set<string>();

  /**
   * Starts a scene action: clears the previous error and marks the action
   * pending. Returns `false` when another scene action is already in flight,
   * which serializes the management operations (double activation is a no-op).
   */
  function beginSceneAction(actionId: string): boolean {
    if (sceneActionPending !== null) return false;
    sceneActionError = null;
    sceneActionPending = actionId;
    return true;
  }

  /** Reports a failed scene action and returns `false` for the caller. */
  function failSceneAction(error: unknown, action: string): false {
    console.warn(`[App] Scene action "${action}" failed:`, error);
    sceneActionError = SCENE_ACTION_ERROR_MESSAGE;
    return false;
  }

  /** Local model of a scene id announced by `SceneList`. */
  function createPlaceholderScene(sceneId: string): Scene {
    return {
      id: sceneId,
      name: sceneId === '/' ? 'メインシーン' : sceneId,
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
  }

  /**
   * Removes a scene from the local model.
   *
   * The root scene `/` always exists server-side and the last remaining scene
   * is kept so the field always has something to render. When the removed
   * scene was current, the selection falls back to `/` (or the first scene).
   * Its cached log records are dropped as well because deleting a scene deletes
   * its logs on the server.
   */
  function removeSceneLocally(sceneId: string): boolean {
    if (sceneId === '/') return false;
    if (scenes.length <= 1) return false;
    if (!scenes.some((scene) => scene.id === sceneId)) return false;

    const next = scenes.filter((scene) => scene.id !== sceneId);
    scenes = next;
    pendingSceneListIds.delete(sceneId);
    chunkManager.clearSceneRecords(sceneId);
    if (currentSceneId === sceneId) {
      currentSceneId = (next.find((scene) => scene.id === '/') ?? next[0]).id;
    }
    return true;
  }

  /**
   * Applies a `SceneList` frame (Issue #109 / sub-issue #143, Issue #159).
   *
   * The list is authoritative in both directions: it adds unknown scenes,
   * removes scenes the server no longer lists, and re-points `currentSceneId`
   * when the current scene vanished. Entries are `SceneSummary` objects (new
   * protocol) or bare ids (older servers); the summary `name` is
   * server-authoritative and overwrites local placeholders. `/` and scenes
   * still waiting for a list that includes a fresh `SceneCreated` id are kept.
   */
  function applySceneList(rawEntries: unknown) {
    if (!Array.isArray(rawEntries) || rawEntries.length === 0) return;
    const summaries = rawEntries
      .map((entry) => sceneListEntrySummary(entry))
      .filter((summary): summary is SceneSummary => summary !== null);
    if (summaries.length === 0) return;

    const summariesById = new Map(summaries.map((summary) => [summary.id, summary]));
    const serverIds = new Set(summariesById.keys());
    const next: Scene[] = [];
    let changed = false;

    for (const scene of scenes) {
      if (serverIds.has(scene.id)) {
        pendingSceneListIds.delete(scene.id);
        const summary = summariesById.get(scene.id);
        const name =
          summary && typeof summary.name === 'string' && summary.name.trim().length > 0
            ? summary.name
            : undefined;
        if (name && scene.name !== name) scene.name = name;
        next.push(scene);
        continue;
      }
      // Never drop the root scene nor a scene this client has just created and
      // the server has already acknowledged.
      if (scene.id === '/' || pendingSceneListIds.has(scene.id)) {
        next.push(scene);
        continue;
      }
      changed = true;
    }

    for (const summary of summaries) {
      if (next.some((scene) => scene.id === summary.id)) continue;
      const placeholder = createPlaceholderScene(summary.id);
      if (typeof summary.name === 'string' && summary.name.trim().length > 0) {
        placeholder.name = summary.name;
      }
      next.push(placeholder);
      changed = true;
    }

    if (changed) scenes = next;

    if (!next.some((scene) => scene.id === currentSceneId)) {
      const fallback = next.find((scene) => scene.id === '/') ?? next[0];
      if (fallback) {
        currentSceneId = fallback.id;
        applyPieceStatesToCharacters(currentSceneId);
        roomSocket?.enterScene(fallback.id);
      }
    }
  }

  $effect(() => {
    if (!roomSocket) return;
    roomSocket.setChunkLogManager(chunkManager);
    const unsubscribe = roomSocket.subscribe((frame) => {
      if (typeof frame !== 'object' || frame === null) return;
      if ('SceneList' in frame) {
        applySceneList(frame.SceneList.scenes);
      }
      // `SceneInfo` carries the server-authoritative name for create / rename
      // broadcasts so other clients update without a reload (Issue #159).
      if ('SceneInfo' in frame) {
        applySceneSummary(frame.SceneInfo);
      }
      // Second safeguard for a deletion: the broadcast also removes the scene
      // even when this client did not issue the `DeleteScene` request.
      if ('SceneDeleted' in frame) {
        removeSceneLocally(String(frame.SceneDeleted.scene_id));
      }
      if ('CharacterList' in frame) {
        const charList: Character[] = [];
        if (frame.CharacterList?.characters) {
          for (const [id, raw] of Object.entries(frame.CharacterList.characters)) {
            charList.push(parseCharacter(id, raw));
          }
        }
        // The server list is authoritative: replace even when it is empty
        // (Issue #109 / #144) and keep the selection valid.
        applyCharacterList(charList);
        applyPieceStatesToCharacters(currentSceneId);
      }
      if ('CharacterDeleted' in frame) {
        removeCharacterLocally(String(frame.CharacterDeleted.character_id));
      }
      // Snapshot application is registered both as the `onStateSnapshot`
      // callback (real socket) and here (custom / test sockets). Every supported
      // key is applied idempotently, so a duplicate snapshot is harmless.
      if ('StateSnapshot' in frame) {
        const { scene_id, states } = frame.StateSnapshot;
        applyStateSnapshot(scene_id, tinToJsonMap(states));
      }
      if ('StateUpdated' in frame) {
        const { scene_id, key, value } = frame.StateUpdated;
        const targetScene = scenes.find((s) => s.id === scene_id);
        if (targetScene) {
          // BGM keys (`std::bgm`, `std::bgm<N>`, `bgm::<slot>::volume`), the
          // scene name, the background reference / dimensions, the view
          // transform and piece placement all share one idempotent path.
          applySceneStateEntry(targetScene, key, value);
        }
      }
      if ('StateDeleted' in frame) {
        const { scene_id, key } = frame.StateDeleted;
        // Resolve the target scene from the frame itself: the previous version
        // referenced a `targetScene` that only existed inside the StateUpdated
        // branch, which threw a ReferenceError on every StateDeleted frame.
        const targetScene = scenes.find((s) => s.id === scene_id);
        if (targetScene) {
          applySceneStateDeleted(targetScene, key);
        }
      }
      if ('Error' in frame) {
        if (
          frame.Error.message?.includes('全体公開されていない部屋です') ||
          frame.Error.code === '403' ||
          frame.Error.code === 'FORBIDDEN'
        ) {
          isForbidden = true;
        }
      }
    });
    return unsubscribe;
  });

  function handleEnterRoom(room: VisitedRoom | RecentRoom) {
    const roomId = (room as any).roomId || room.id;
    const roomName = (room as any).roomName || room.name;
    const roomUrl = (room as any).url || (room as any).roomUrl;
    const visitedAt = (room as any).visitedAt || room.lastVisited || Date.now();
    const normalizedRoom: VisitedRoom = {
      id: roomId,
      name: roomName,
      url: roomUrl,
      lastVisited: visitedAt,
      isAdmin: room.isAdmin,
      ...room,
    };
    saveRecentRoom({
      roomId,
      roomName,
      url: roomUrl,
      roomUrl,
      visitedAt,
      thumbnailUrl: (room as any).thumbnailUrl,
      ownerAccountId: (room as any).ownerAccountId,
      isAdmin: room.isAdmin,
    });
    currentRoom = normalizedRoom;
  }

  function handleSelectScene(sceneId: string) {
    if (scenes.some((s) => s.id === sceneId)) {
      currentSceneId = sceneId;
      // Restore the placement hydrated for this scene (Issue #159). The
      // entering `StateSnapshot` refreshes it a moment later.
      applyPieceStatesToCharacters(sceneId);
      roomSocket?.enterScene(sceneId);
    }
  }

  /**
   * Creates a scene and selects it (Issue #109 / sub-issue #143, Issue #159).
   *
   * The name is written to `std::name` atomically with the creation, so the
   * `SceneCreated` ack already carries the authoritative summary. Without a
   * connected socket the action is reported as an error and the local scene
   * list is left untouched: an offline mutation that only exists locally would
   * look like success and then vanish on reconnect.
   */
  async function handleCreateScene(name: string, inheritJuice: boolean): Promise<boolean | void> {
    if (isViewer) return;
    if (!beginSceneAction('create')) return;

    try {
      const newId = `scene-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      const juices = inheritJuice ? inheritJuices(currentScene.juices) : createDefaultJuices();
      const socket = roomSocket;

      if (!socket || !socket.isConnected) {
        throw new Error('ルームに接続していないため、シーンを作成できません');
      }

      await socket.requestCreateScene(newId, inheritJuice, name);

      pendingSceneListIds.add(newId);
      const alreadyListed = scenes.find((scene) => scene.id === newId);
      if (alreadyListed) {
        // A `SceneList` broadcast created the placeholder while we were waiting.
        alreadyListed.name = name;
        alreadyListed.juices = juices;
      } else {
        scenes = [
          ...scenes,
          { id: newId, name, juices, viewTransform: { x: 0, y: 0, scale: 1 }, members: [] },
        ];
      }
      currentSceneId = newId;
      applyPieceStatesToCharacters(newId);
      return true;
    } catch (error) {
      return failSceneAction(error, 'create');
    } finally {
      sceneActionPending = null;
    }
  }

  /**
   * Deletes a scene (Issue #109 / sub-issue #143, Issue #159).
   *
   * The "keep at least one scene" guard is unchanged. The removal waits for the
   * `SceneDeleted` ack; without a connected socket the action fails and the
   * local list is left untouched instead of pretending the deletion succeeded.
   */
  async function handleDeleteScene(sceneId: string): Promise<boolean | void> {
    if (isViewer) return;
    if (scenes.length <= 1) return; // Keep at least one scene
    if (!beginSceneAction('delete-scene')) return;

    try {
      const socket = roomSocket;
      if (!socket || !socket.isConnected) {
        throw new Error('ルームに接続していないため、シーンを削除できません');
      }

      await socket.requestDeleteScene(sceneId);
      removeSceneLocally(sceneId);
      return true;
    } catch (error) {
      return failSceneAction(error, 'delete-scene');
    } finally {
      sceneActionPending = null;
    }
  }

  /**
   * Deletes every log of a scene (Issue #109 / sub-issue #143, Issue #159).
   *
   * The local chunk cache is cleared only after `requestRewindLog(sceneId, 0, 0)`
   * is acknowledged. Without a connected socket the action fails and the cache
   * is kept, so an offline "success" cannot silently drop the conversation.
   */
  async function handleDeleteSceneLogs(sceneId: string): Promise<boolean | void> {
    if (!beginSceneAction('delete-logs')) return;

    try {
      const socket = roomSocket;
      if (!socket || !socket.isConnected) {
        throw new Error('ルームに接続していないため、ログを削除できません');
      }
      await socket.requestRewindLog(sceneId, 0, 0);
      chunkManager.clearSceneRecords(sceneId);
      if (currentRoom?.id) {
        persistRoomLogs(currentRoom.id);
      }
      return true;
    } catch (error) {
      return failSceneAction(error, 'delete-logs');
    } finally {
      sceneActionPending = null;
    }
  }

  /** Commits an acknowledged member move to `scenes`, `currentSceneId` and `movements`. */
  function commitMemberMove(
    characterId: string,
    fromSceneId: string,
    result: ReturnType<typeof moveMemberToScene>,
  ) {
    scenes = result.updatedScenes;
    currentSceneId = result.targetScene.id;

    const char = characters.find((c) => c.id === characterId);
    movements.push({
      characterId,
      characterName: char?.name || 'Unknown',
      fromSceneId,
      toSceneId: result.targetScene.id,
      timestamp: Math.floor(Date.now() / 1000),
    });
  }

  /**
   * Moves a character to another scene (Issue #109 / sub-issue #143, #159).
   *
   * The target scene id (and the implicitly created scene for an unknown name)
   * is computed before the request because `MoveCharacter` carries it, but the
   * local model is only replaced after the ack. Without a connected socket the
   * move fails instead of mutating the local model. The room-state mirror
   * `character::<id>::scene` is kept and is best effort: the `CharacterMoved`
   * ack is authoritative, so a rejected state write only logs a warning.
   */
  async function handleMoveMember(
    characterId: string,
    targetSceneName: string,
  ): Promise<boolean | void> {
    if (isViewer) return;
    if (!beginSceneAction(`move:${characterId}`)) return;

    try {
      const fromSceneId =
        (scenes.find((scene) => scene.members.includes(characterId)) || currentScene).id;
      const result = moveMemberToScene(characterId, targetSceneName, currentScene, scenes);
      const socket = roomSocket;
      const numericCharId = Number(characterId);

      if (!socket || !socket.isConnected) {
        throw new Error('ルームに接続していないため、メンバーを移動できません');
      }

      if (!Number.isNaN(numericCharId)) {
        await socket.requestMoveCharacter(numericCharId, result.targetScene.id);
      }
      try {
        await socket.requestUpdateState(
          result.targetScene.id,
          `character::${characterId}::scene`,
          result.targetScene.id,
        );
      } catch (stateError) {
        console.warn('[App] Failed to mirror the member move into the room state:', stateError);
      }

      commitMemberMove(characterId, fromSceneId, result);
      return true;
    } catch (error) {
      return failSceneAction(error, 'move');
    } finally {
      sceneActionPending = null;
    }
  }

  function handleTogglePiece(characterId: string) {
    if (isViewer) return;
    characters = characters.map((c) => {
      if (c.id === characterId) {
        const nextPlaced = !c.placedAsPiece;
        // Keep the per-scene hydrate map in sync so a later `CharacterList`
        // re-application does not undo this toggle.
        const sceneId = currentScene.id;
        let states = pieceStatesByScene.get(sceneId);
        if (!states) {
          states = new Map();
          pieceStatesByScene.set(sceneId, states);
        }
        states.set(characterId, { ...(states.get(characterId) ?? {}), placed: nextPlaced });
        // RoomSocket de-duplicates state frames and replays the final value on
        // reconnect; without a socket the change stays local only.
        roomSocket?.updateState(currentScene.id, `piece::${characterId}::placed`, nextPlaced);
        return {
          ...c,
          placedAsPiece: nextPlaced,
          piecePosition: c.piecePosition || { x: 200, y: 200 },
        };
      }
      return c;
    });
  }

  function handlePieceMove(characterId: string, position: { x: number; y: number }) {
    if (isViewer) return;
    characters = characters.map((c) => {
      if (c.id === characterId) {
        return {
          ...c,
          piecePosition: { ...position },
        };
      }
      return c;
    });
    const sceneId = currentScene.id;
    let states = pieceStatesByScene.get(sceneId);
    if (!states) {
      states = new Map();
      pieceStatesByScene.set(sceneId, states);
    }
    states.set(characterId, { ...(states.get(characterId) ?? {}), pos: { ...position } });
    roomSocket?.updateState(currentScene.id, `piece::${characterId}::pos`, position);
  }

  /**
   * Legacy optimistic create (no server ack), kept for the tab's standalone
   * inline editor fallback. The hoisted editor goes through
   * {@link handleSaveCharacter} instead.
   */
  function handleCreateCharacter(char: Character) {
    if (isViewer) return;
    characters.push(char);
    if (!currentScene.members.includes(char.id)) {
      currentScene.members.push(char.id);
    }
    const numId = Number(char.id);
    roomSocket?.createCharacter(Number.isNaN(numId) ? Date.now() : numId, char);
  }

  /** Legacy optimistic update, kept for the tab's standalone inline editor fallback. */
  function handleUpdateCharacter(char: Character) {
    if (isViewer) return;
    characters = characters.map((c) => (c.id === char.id ? char : c));
    const numId = Number(char.id);
    roomSocket?.updateCharacter(Number.isNaN(numId) ? 0 : numId, char);
  }

  /**
   * Saves the hoisted editor's character (Issue #109 / sub-issue #144).
   *
   * - No room id and no socket: legacy local create/update (standalone tests).
   * - Connected: the panel is disabled (`characterSaving`) and the save awaits
   *   the server ack. Create applies the server-allocated id; update replaces
   *   the record by id. The panel closes only on success.
   * - Timeout / retryable ack failure: the operation is enqueued in the durable
   *   `kind: 'character'` outbox (30-day TTL), the panel closes and the tab
   *   flips to its queueing state via `pendingCharacterCount`.
   * - `PermissionDenied` / other non-retryable failure: the panel stays open
   *   with `characterSaveError`.
   * - Offline but in a room: enqueue directly, never through `RoomSocket`'s
   *   in-memory queue (it would resend the frame on reconnect and double send).
   */
  async function handleSaveCharacter(updated: Character): Promise<void> {
    if (isViewer) return;
    if (characterSaving) return; // guard against a double submit

    const isCreate = characterEditIsNew;
    const roomId = getActiveRoomId();
    const socket = roomSocket;
    const isConnected = !!socket && socket.isConnected;

    characterSaveError = null;

    // Mounted without a room or a socket (tests / standalone): local only.
    if (!roomId && !socket) {
      if (isCreate) applyCreatedCharacter(updated, updated.id);
      else applyUpdatedCharacter(updated);
      closeCharacterEditPanel();
      return;
    }

    characterSaving = true;
    try {
      if (!socket || !isConnected) {
        if (!roomId) {
          // A socket exists but there is no room to key an outbox entry by.
          if (isCreate) applyCreatedCharacter(updated, updated.id);
          else applyUpdatedCharacter(updated);
          closeCharacterEditPanel();
          return;
        }
        // Offline / no socket: straight to the durable outbox.
        const queued = await queueCharacterOperation(roomId, {
          operation: isCreate ? 'create' : 'update',
          character: updated,
        });
        if (!queued) {
          characterSaveError = CHARACTER_SAVE_FAILED_MESSAGE;
          return;
        }
        closeCharacterEditPanel();
        return;
      }

      try {
        if (isCreate) {
          const ack = await socket.requestCreateCharacter(updated);
          applyCreatedCharacter(updated, serverCharacterId(ack, updated.id));
        } else {
          const ack = await socket.requestUpdateCharacter(Number(updated.id), updated);
          applyUpdatedCharacter(updated, serverCharacterId(ack, updated.id));
        }
        closeCharacterEditPanel();
      } catch (err) {
        if (isRetryableCharacterError(err) && roomId) {
          // The frame may have left the socket, but no ack arrived: replay it
          // from the outbox on reconnect instead of losing the edit.
          const queued = await queueCharacterOperation(roomId, {
            operation: isCreate ? 'create' : 'update',
            character: updated,
          });
          if (!queued) {
            characterSaveError = CHARACTER_SAVE_FAILED_MESSAGE;
            return;
          }
          closeCharacterEditPanel();
          return;
        }
        console.error('Failed to save the character:', err);
        characterSaveError = isPermissionDeniedError(err)
          ? CHARACTER_PERMISSION_DENIED_MESSAGE
          : CHARACTER_SAVE_FAILED_MESSAGE;
      }
    } finally {
      characterSaving = false;
    }
  }

  /**
   * Deletes a character after the server acknowledgement (Issue #109 / #144).
   *
   * - Connected: `requestDeleteCharacter` first; the row disappears only on ack.
   * - Offline / timeout in a room: a `delete` operation is queued and the row is
   *   kept locally — the server still has it, and the flush removes it on the
   *   next connect (a local removal would resurrect on the next `CharacterList`).
   * - No room / no socket: local removal (legacy standalone behaviour).
   */
  async function handleDeleteCharacter(charId: string) {
    if (isViewer) return;

    const roomId = getActiveRoomId();
    const socket = roomSocket;
    const isConnected = !!socket && socket.isConnected;

    if (!socket || !isConnected) {
      if (!roomId) {
        removeCharacterLocally(charId);
        return;
      }
      const queued = await queueCharacterOperation(roomId, {
        operation: 'delete',
        characterId: charId,
      });
      if (!queued) console.error('Failed to queue the character deletion:', charId);
      return;
    }

    try {
      await socket.requestDeleteCharacter(Number(charId));
      removeCharacterLocally(charId);
    } catch (err) {
      if (isRetryableCharacterError(err) && roomId) {
        const queued = await queueCharacterOperation(roomId, {
          operation: 'delete',
          characterId: charId,
        });
        if (!queued) console.error('Failed to queue the character deletion:', charId);
        return;
      }
      console.error('Failed to delete the character:', err);
    }
  }

  function handleTransformChange(id: string, t: ViewTransform) {
    if (isViewer) return;
    const sc = scenes.find((s) => s.id === id);
    if (sc) sc.viewTransform = t;
    roomSocket?.updateState(id, 'view::transform', t);
  }

  // ---------------------------------------------------------------------------
  // Extension synchronization (Issue #109 / sub-issue #145)
  // ---------------------------------------------------------------------------

  /**
   * Replaces the extension list after the tab got every server acknowledgement.
   *
   * `extensions` stays the single source of truth: the tab never mutates it
   * directly, and this handler is the only writer for server-backed changes.
   * Pipeline registrations of deleted ids are dropped so a removed extension
   * cannot keep executing from a stale instance.
   */
  function handleUpdateExtensions(next: ExtensionItem[]): void {
    if (!Array.isArray(next)) return;
    extensionStateApplied = true;

    const nextIds = new Set(next.map((ext) => ext.id));
    for (const previous of extensions) {
      if (!nextIds.has(previous.id)) {
        extensionPipeline.unregisterInstance(previous.id);
      }
    }
    extensions = next;
  }

  /**
   * Registers a downloaded / uploaded wasm instance with the pipeline so the
   * extension can actually run even when its bytes never came from
   * `loadDefaultExtensions`.
   */
  async function handleRegisterExtension(id: string, wasmBytes: Uint8Array): Promise<void> {
    try {
      const instance = await WasmExtensionInstance.instantiate(wasmBytes);
      extensionPipeline.registerInstance(id, instance);
    } catch (err) {
      console.error(`Failed to instantiate the extension "${id}":`, err);
    }
  }

  /**
   * Sends a chat message (Issue #109 / sub-issue #139).
   *
   * The record is rendered only after the server's `LogEventAppended` echo:
   * - connected: `requestAppendLog` resolves on the echo, and `RoomSocket`
   *   appends that echo into `chunkManager` before notifying subscribers, so
   *   this function never appends the record itself on success.
   * - offline / no socket: the message goes straight to the durable outbox
   *   instead of `RoomSocket.send()`, because `RoomSocket` also queues the frame
   *   internally and would resend it on reconnect (double delivery).
   * - timeout / connection failure on a connected socket: the frame already
   *   left the socket, so queueing it in the outbox cannot duplicate a queued
   *   frame; the entry is replayed on reconnect.
   * - no room id at all (e.g. `App` mounted without a room in tests): there is
   *   nowhere to key an outbox entry, so the record is appended locally and
   *   treated as local-only.
   */
  /**
   * Requests a room `done` change and resolves only after the banquette ack.
   *
   * `CommonHeader` keeps its confirmation dialog open until this resolves, so a
   * failed or denied request never looks like success.
   */
  async function handleSetDone(done: boolean): Promise<void> {
    if (!roomSocket) {
      throw new Error('ルームに接続していません');
    }
    await roomSocket.requestSetDone(done);
  }

  async function handleSendMessage(text: string, characterId?: string) {
    if (isViewer) return;
    if (isSending) return; // prevent double sends while an echo is pending

    const char = characters.find((c) => c.id === characterId);
    const speaker = char?.name || 'Anonymous';
    const sceneId = currentScene.id;
    const record: LogRecord = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      sceneId,
      content: text,
      speaker,
      speakerId: characterId,
      timestamp: Math.floor(Date.now() / 1000),
    };
    const roomId = getActiveRoomId();

    isSending = true;
    try {
      if (!roomSocket || !roomSocket.isConnected) {
        // Offline: enqueue durably, never via RoomSocket's in-memory queue.
        if (roomId) {
          await enqueueOfflineMessage(roomId, sceneId, record);
        } else {
          chunkManager.appendRecord(record);
        }
        return;
      }

      try {
        await roomSocket.requestAppendLog(sceneId, record);
        // Success: the echo already appended and persisted the record.
      } catch (err) {
        const isPermissionDenied =
          err instanceof ServerOperationError && err.code === 'PermissionDenied';
        const isRetryable =
          err instanceof OperationTimeoutError ||
          (err instanceof ServerOperationError && !isPermissionDenied);
        if (!isRetryable) throw err;
        if (roomId) {
          await enqueueOfflineMessage(roomId, sceneId, record);
        } else {
          // Connected but no room to key an outbox entry by.
          chunkManager.appendRecord(record);
        }
      }
    } catch (err) {
      console.error('Failed to send the chat message:', err);
    } finally {
      isSending = false;
      if (extensions.some((ext) => ext.enabled && ext.wasmBytes)) {
        const initialBytes = new TextEncoder().encode(text);
        extensionPipeline.executePipeline(extensions, initialBytes).catch(() => {
          // Keep raw message displayed if extension pipeline fails
        });
      }
    }
  }
</script>

<div class="alhalte-app-root" data-testid="alhalte-app">
  <!-- Server connection / offline detection indicator placed in flow above panels on y-axis (Issue #111) -->
  <OfflineIndicator
    connectionState={currentConnectionState}
    {debugMode}
    {debugAlwaysShow}
    onReconnect={handleReconnectFromIndicator}
  />

  {#if isForbidden}
    <GateScreen onReturnHome={returnHome} />
  {:else if preflightStatus === 'not-found'}
    <div class="room-state-screen" data-testid="room-not-found">
      <h1 class="room-state-title">部屋が見つかりません</h1>
      <p class="room-state-message">
        この部屋は存在しないか、削除されています。招待リンクを確認してください。
      </p>
      <Button variant="filled" onclick={returnHome} data-testid="room-not-found-home-button">
        ホームに戻る
      </Button>
    </div>
  {:else if !currentRoom}
    <HomeScreen onEnterRoom={handleEnterRoom} onSelectRoom={handleEnterRoom} {fetchFn} />
  {:else if preflightStatus === 'checking' || preflightStatus === 'idle'}
    <div class="room-state-screen" data-testid="room-preflight-loading" role="status">
      <p class="room-state-message">部屋を確認しています…</p>
    </div>
  {:else if preflightStatus === 'error'}
    <div class="room-state-screen" data-testid="room-preflight-error" role="alert">
      <h1 class="room-state-title">部屋に入室できませんでした</h1>
      <p class="room-state-message">
        {preflightError ?? 'サーバーと通信できませんでした。'}
      </p>
      <div class="room-state-actions">
        <Button variant="filled" onclick={handleRetryPreflight} data-testid="room-preflight-retry">
          再試行
        </Button>
        <Button variant="text" onclick={returnHome} data-testid="room-preflight-home-button">
          ホームに戻る
        </Button>
      </div>
    </div>
  {:else}
    <div
      class="room-layout-container"
      class:landscape={isLandscape}
      class:portrait={!isLandscape}
      data-testid="room-layout"
    >
      {#if isLandscape}
        <!-- Landscape layout:
          左側にスクリーンパネルや駒などを置くフィールドパネル、右側にSide Sheetでコントロールパネル。
        -->
        <div class="field-panel-wrapper landscape-field" data-testid="landscape-field-wrapper">
          <FieldPanel
            roomName={currentRoom.name}
            scene={currentScene}
            {characters}
            onTransformChange={handleTransformChange}
            onPieceMove={handlePieceMove}
          />
        </div>

        <div
          class="control-panel-wrapper landscape-control"
          class:collapsed={!isControlSheetOpen}
          data-testid="landscape-control-wrapper"
        >
          <Sheet
            bind:open={isControlSheetOpen}
            placement="end"
            modal={false}
            class={['landscape-side-sheet', isResizing && 'is-resizing']}
            style="--np-sheet-size: {sideSheetWidth}px; width: {isControlSheetOpen ? `${sideSheetWidth}px` : '0px'}; min-width: {isControlSheetOpen ? '10vw' : '0'};"
            data-testid="landscape-side-sheet"
          >
            <div class="side-sheet-inner-container" class:resizing={isResizing}>
              <!-- Stepless Resize Bar / Drag Handle on the left edge of the side sheet -->
              <div
                class="side-sheet-resize-bar"
                data-testid="side-sheet-resize-handle"
                role="separator"
                aria-orientation="vertical"
                aria-label="サイドシート幅リサイズ"
                onpointerdown={handleSideResizeDown}
              >
                <div class="side-sheet-resize-pill"></div>
              </div>

              <!-- Main Control Panel -->
              <div class="side-sheet-content-wrapper">
                <ControlPanel
                  {currentScene}
                  allScenes={scenes}
                  {characters}
                  {extensions}
                  {extensionPipeline}
                  {movements}
                  {accessibleSceneIds}
                  {selectedCharacterId}
                  {isAdmin}
                  {isViewer}
                  sending={isSending}
                  {pendingMessageCount}
                  {sceneActionPending}
                  {sceneActionError}
                  {chunkManager}
                  {roomSocket}
                  connectionState={currentConnectionState}
                  onCollapse={() => (isControlSheetOpen = false)}
                  onSelectScene={handleSelectScene}
                  onSelectCharacter={(id) => (selectedCharacterId = id)}
                  onUpdateScene={handleUpdateScene}
                  onCreateScene={handleCreateScene}
                  onDeleteScene={handleDeleteScene}
                  onMoveMember={handleMoveMember}
                  onUpdateCharacter={handleUpdateCharacter}
                  onCreateCharacter={handleCreateCharacter}
                  onDeleteCharacter={handleDeleteCharacter}
                  onTogglePiece={handleTogglePiece}
                  onUpdateExtensions={handleUpdateExtensions}
                  onRegisterExtension={handleRegisterExtension}
                  roomKey={extensionRoomKey}
                  onSendMessage={handleSendMessage}
                  onRequestDeleteScene={handleRequestDeleteScene}
                  onRequestDeleteSceneLogs={handleRequestDeleteSceneLogs}
                  onDeleteSceneLogs={handleDeleteSceneLogs}
                  onRequestDeleteCharacter={handleRequestDeleteCharacter}
                  roomId={currentRoom.id || (currentRoom as any).roomId}
                  onManagePermissions={() => (isPermissionModalOpen = true)}
                  onSetDone={handleSetDone}
                  onRequestAssetPicker={handleRequestAssetPicker}
                  onRequestEditCharacter={handleRequestEditCharacter}
                  onRequestCreateCharacter={handleRequestCreateCharacter}
                  onRefreshCharacters={applyCharacterList}
                  {pendingCharacterCount}
                />
              </div>
            </div>
          </Sheet>

          {#if !isControlSheetOpen}
            <div class="reopen-sheet-floating landscape-reopen" data-testid="side-sheet-toggle-button">
              <IconButton
                variant="tonal"
                onclick={() => (isControlSheetOpen = true)}
                data-testid="reopen-sheet-button"
                aria-label="コントロールパネルを開く"
              >
                <Icon>chevron_left</Icon>
              </IconButton>
            </div>
          {/if}
        </div>
      {:else}
        <!-- Portrait layout:
          全体にフィールド、その上にBottom Sheetでコントロールパネル。
        -->
        <div class="field-panel-wrapper portrait-field" data-testid="portrait-field-wrapper">
          <FieldPanel
            roomName={currentRoom.name}
            scene={currentScene}
            {characters}
            onTransformChange={handleTransformChange}
            onPieceMove={handlePieceMove}
          />
        </div>

        <!-- Bottom Sheet Control Panel -->
        <div
          class="bottom-sheet-container"
          class:collapsed={!isControlSheetOpen}
          data-testid="portrait-bottom-sheet"
        >
          <Sheet
            bind:open={isControlSheetOpen}
            bind:expanded={isControlSheetExpanded}
            placement="bottom"
            modal={false}
            handle={true}
            handleLabel="シートの開閉"
            class="portrait-bottom-sheet"
            data-testid="portrait-sheet-dialog"
          >
            <div class="bottom-sheet-content-wrapper">
              <ControlPanel
                {currentScene}
                allScenes={scenes}
                {characters}
                {extensions}
                {extensionPipeline}
                {movements}
                {accessibleSceneIds}
                {selectedCharacterId}
                {isAdmin}
                {isViewer}
                sending={isSending}
                {pendingMessageCount}
                {sceneActionPending}
                {sceneActionError}
                {chunkManager}
                {roomSocket}
                connectionState={currentConnectionState}
                onCollapse={() => (isControlSheetOpen = false)}
                onSelectScene={handleSelectScene}
                onSelectCharacter={(id) => (selectedCharacterId = id)}
                onUpdateScene={handleUpdateScene}
                onCreateScene={handleCreateScene}
                onDeleteScene={handleDeleteScene}
                onMoveMember={handleMoveMember}
                onUpdateCharacter={handleUpdateCharacter}
                onCreateCharacter={handleCreateCharacter}
                onDeleteCharacter={handleDeleteCharacter}
                onTogglePiece={handleTogglePiece}
                onUpdateExtensions={handleUpdateExtensions}
                onRegisterExtension={handleRegisterExtension}
                roomKey={extensionRoomKey}
                onSendMessage={handleSendMessage}
                onRequestDeleteScene={handleRequestDeleteScene}
                onRequestDeleteSceneLogs={handleRequestDeleteSceneLogs}
                onDeleteSceneLogs={handleDeleteSceneLogs}
                onRequestDeleteCharacter={handleRequestDeleteCharacter}
                roomId={currentRoom.id || (currentRoom as any).roomId}
                onManagePermissions={() => (isPermissionModalOpen = true)}
                onSetDone={handleSetDone}
                onRequestAssetPicker={handleRequestAssetPicker}
                onRequestEditCharacter={handleRequestEditCharacter}
                onRequestCreateCharacter={handleRequestCreateCharacter}
                onRefreshCharacters={applyCharacterList}
                {pendingCharacterCount}
              />
            </div>
          </Sheet>

          {#if !isControlSheetOpen}
            <div class="reopen-sheet-floating portrait-reopen">
              <IconButton
                variant="tonal"
                onclick={() => {
                  isControlSheetOpen = true;
                  isControlSheetExpanded = false;
                }}
                data-testid="reopen-sheet-button"
                aria-label="コントロールパネルを開く"
              >
                <Icon>bottom_panel_open</Icon>
              </IconButton>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Fullscreen Overlays / Dialogs Layer at Root Viewport Level -->
  {#if activeOverlay?.type === 'delete-scene'}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="root-delete-scene-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) activeOverlay = null;
      }}
    >
      <div
        class="dialog-box warning-box"
        data-testid="graph-delete-confirm-dialog"
      >
        <div data-testid="delete-confirm-dialog">
          <h2 class="dialog-title-centered warning-text" data-testid="graph-delete-dialog-title">
            <span data-testid="delete-dialog-title">シーン「{activeOverlay.sceneName}」を削除しますか？</span>
          </h2>
          <p class="warning-body" data-testid="graph-delete-dialog-warning">
            <span data-testid="delete-dialog-warning">このシーンを削除すると、配置されたログやJuices設定は元に戻せません。</span>
          </p>

          <div class="dialog-footer-right">
            <Button
              variant="text"
              onclick={() => (activeOverlay = null)}
              data-testid="graph-cancel-delete-btn"
            >
              <span data-testid="cancel-delete-btn">キャンセル</span>
            </Button>
            <div class="confirm-delete-wrapper">
              <Button
                variant="filled"
                class="delete-scene-tonal-btn"
                disabled={sceneActionPending !== null}
                onclick={() => {
                  if (activeOverlay?.type === 'delete-scene') {
                    const { sceneId } = activeOverlay;
                    // Close immediately so the spinner shows on the management
                    // icon; a failure appears under the management controls.
                    activeOverlay = null;
                    void handleDeleteScene(sceneId);
                  }
                }}
                data-testid="graph-confirm-delete-btn"
              >
                <span data-testid="confirm-delete-button">削除する</span>
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  {:else if activeOverlay?.type === 'delete-scene-logs'}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="root-delete-logs-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) activeOverlay = null;
      }}
    >
      <div
        class="dialog-box warning-box"
        data-testid="delete-logs-confirm-dialog"
      >
        <h2 class="dialog-title-centered warning-text" data-testid="delete-logs-dialog-title">
          シーン「{activeOverlay.sceneName}」のログを削除しますか？
        </h2>
        <p class="warning-body" data-testid="delete-logs-dialog-warning">
          このシーンのすべての会話ログを削除します。この操作は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button
            variant="text"
            onclick={() => (activeOverlay = null)}
            data-testid="cancel-delete-logs-btn"
          >
            キャンセル
          </Button>
          <div class="confirm-delete-wrapper">
            <Button
              variant="filled"
              class="delete-scene-tonal-btn"
              disabled={sceneActionPending !== null}
              onclick={() => {
                if (activeOverlay?.type === 'delete-scene-logs') {
                  const { sceneId } = activeOverlay;
                  activeOverlay = null;
                  void handleDeleteSceneLogs(sceneId);
                }
              }}
              data-testid="confirm-delete-logs-button"
            >
              削除する
            </Button>
          </div>
        </div>
      </div>
    </div>
  {:else if activeOverlay?.type === 'delete-character'}
    <div
      class="fullscreen-warning-overlay"
      data-testid="root-delete-char-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) activeOverlay = null;
      }}
    >
      <div class="delete-warning-box" data-testid="char-delete-confirm-dialog">
        <h2 class="warning-title-centered" data-testid="char-delete-warning-title">
          キャラクター「{activeOverlay.characterName}」を削除しますか？
        </h2>
        <p class="warning-desc-centered" data-testid="char-delete-warning-desc">
          このキャラクターを削除すると、チャットパレットやステータス情報を含め、すべて消去されます。元に戻すことはできません。
        </p>

        <div class="warning-actions">
          <Button variant="text" onclick={() => (activeOverlay = null)} data-testid="char-cancel-delete-btn">キャンセル</Button>
          <button
            type="button"
            class="danger-confirm-btn"
            onclick={() => {
              if (activeOverlay?.type === 'delete-character') {
                void handleDeleteCharacter(activeOverlay.characterId);
                activeOverlay = null;
              }
            }}
            data-testid="confirm-char-delete-button"
          >
            削除する
          </button>
        </div>
      </div>
    </div>
  {:else if activeOverlay?.type === 'asset-picker'}
    <div
      class="root-asset-picker-wrapper"
      data-testid="root-asset-picker-wrapper"
    >
      <AssetPickerModal
        open={true}
        initialMediaType={activeOverlay.mediaType}
        initialScope={activeOverlay.initialScope}
        {isAdmin}
        {roomSocket}
        roomKey={extensionRoomKey}
        roomId={currentRoom?.id}
        onSelect={(url, asset) => {
          activeOverlay?.onSelect(url, asset);
          activeOverlay = null;
        }}
        onClose={() => {
          activeOverlay = null;
        }}
      />
    </div>
  {/if}

  <!-- Character Edit Modal at Root Viewport Level (Issue #60).
       Save state machine: disabled panel + wavy spinner while the server
       acknowledges; close only on success or after the operation was queued
       (Issue #109 / #144). -->
  {#if activeCharacterEdit}
    <div
      class="root-character-edit-wrapper"
      data-testid="root-character-edit-wrapper"
    >
      <CharacterEditModal
        character={activeCharacterEdit}
        {isAdmin}
        saving={characterSaving}
        error={characterSaveError}
        onRequestAssetPicker={handleRequestAssetPicker}
        onSave={handleSaveCharacter}
        onClose={handleCloseCharacterEdit}
      />
    </div>
  {/if}

  <!-- Room Permission Management Modal (Issue #94) -->
  {#if isPermissionModalOpen && (currentRoom?.id || (currentRoom as any)?.roomId)}
    <RoomPermissionModal
      roomId={currentRoom.id || (currentRoom as any).roomId}
      initialPermissions={roomPermissions}
      isOpen={isPermissionModalOpen}
      onClose={() => (isPermissionModalOpen = false)}
      onPermissionsChange={(perms) => {
        roomPermissions = perms;
        const aidStr = getSavedAccountId();
        const aid = aidStr ? Number(aidStr) : 0;
        const myPerm = aid ? perms.find((p) => p.account_id === aid) : undefined;
        if (myPerm) {
          userRole = myPerm.role;
        }
      }}
    />
  {/if}
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: Roboto, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background-color: var(--np-color-background);
    color: var(--np-color-on-background);
  }
  .alhalte-app-root {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    position: relative;
    background-color: var(--np-color-background);
    color: var(--np-color-on-background);
    display: flex;
    flex-direction: column;
  }
  .room-state-screen {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 24px;
    box-sizing: border-box;
    text-align: center;
    background-color: var(--np-color-background);
    color: var(--np-color-on-background);
  }
  .room-state-title {
    margin: 0;
    font-size: clamp(1.4rem, 3vw, 2rem);
    font-weight: 800;
    color: var(--np-color-on-surface);
  }
  .room-state-message {
    margin: 0;
    max-width: 560px;
    font-size: 0.95rem;
    line-height: 1.6;
    color: var(--np-color-on-surface-variant);
  }
  .room-state-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .room-layout-container {
    width: 100%;
    display: flex;
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .room-layout-container.landscape {
    flex-direction: row;
  }
  .landscape-field {
    flex: 1;
    height: 100%;
    position: relative;
  }
  .landscape-control {
    display: flex;
    height: 100%;
    position: relative;
    z-index: 200;
    transition: width var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1));
  }
  .landscape-control :global(.landscape-side-sheet) {
    height: 100%;
    border-left: 1px solid var(--np-color-outline-variant);
    box-shadow: var(--np-elevation-2);
    overflow: hidden;
    transition:
      width var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)),
      min-width var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)),
      border-color var(--np-motion-expressive-fast-effects, 200ms ease);
  }
  .landscape-control:not(.collapsed) :global(.landscape-side-sheet) {
    min-width: 10vw;
  }
  .landscape-control :global(.landscape-side-sheet.is-resizing) {
    transition: none !important;
  }
  .landscape-control.collapsed :global(.landscape-side-sheet) {
    width: 0 !important;
    min-width: 0 !important;
    border-left: none !important;
    box-shadow: none !important;
  }
  .landscape-control :global(.landscape-side-sheet .np-sheet-content) {
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: row;
    height: 100%;
    width: 100%;
  }
  .side-sheet-inner-container {
    display: flex;
    flex-direction: row;
    height: 100%;
    width: 100%;
    overflow: hidden;
    position: relative;
  }
  .side-sheet-inner-container.resizing {
    user-select: none;
  }
  .side-sheet-inner-container.resizing :global(*) {
    user-select: none !important;
  }
  .side-sheet-resize-bar {
    width: 20px;
    height: 100%;
    position: relative;
    cursor: col-resize;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    background: var(--np-color-surface-container);
    border-right: 1px solid var(--np-color-outline-variant);
    flex-shrink: 0;
    touch-action: none;
    user-select: none;
    z-index: 10;
    transition: background var(--np-motion-expressive-fast-effects, 150ms ease);
  }
  .side-sheet-resize-bar:hover,
  .side-sheet-inner-container.resizing .side-sheet-resize-bar {
    background: var(--np-color-surface-container-high);
  }
  .side-sheet-resize-pill {
    width: 4px;
    height: 32px;
    border-radius: var(--np-shape-corner-full, 9999px);
    background: var(--np-color-on-surface-variant);
    opacity: 0.6;
    transition: opacity 150ms ease, height 150ms ease;
  }
  .side-sheet-resize-bar:hover .side-sheet-resize-pill,
  .side-sheet-inner-container.resizing .side-sheet-resize-pill {
    opacity: 1;
    height: 48px;
    background: var(--np-color-primary);
  }
  .side-sheet-content-wrapper {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .reopen-sheet-floating.landscape-reopen {
    position: fixed;
    top: 16px;
    right: 16px;
    z-index: 1500;
    animation: reopenFadeIn var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)) forwards;
  }
  @keyframes reopenFadeIn {
    from {
      opacity: 0;
      transform: translateX(20px) scale(0.9);
    }
    to {
      opacity: 1;
      transform: translateX(0) scale(1);
    }
  }
  .room-layout-container.portrait {
    flex-direction: column;
  }
  .portrait-field {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
  }
  .bottom-sheet-container {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: 500;
  }
  .bottom-sheet-container :global(.portrait-bottom-sheet) {
    --np-sheet-size: 50vh;
    --np-sheet-max-width: 100%;
    width: 100% !important;
    max-width: 100% !important;
    inset-inline: 0 !important;
    margin-inline: 0 !important;
    box-shadow: var(--np-elevation-3);
    transition:
      translate var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)),
      max-height var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)),
      display var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)) allow-discrete,
      overlay var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)) allow-discrete;
  }
  .bottom-sheet-container :global(.portrait-bottom-sheet.np-sheet-expanded) {
    max-height: calc(100dvh - 3.5rem);
  }
  .bottom-sheet-container :global(.portrait-bottom-sheet .np-sheet-content) {
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .bottom-sheet-content-wrapper {
    height: 100%;
    max-height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: hidden;
  }
  .reopen-sheet-floating.portrait-reopen {
    position: fixed;
    bottom: 16px;
    right: 16px;
    z-index: 550;
  }
  .fullscreen-dialog-overlay,
  .fullscreen-warning-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9000;
  }
  .dialog-box {
    background: var(--np-color-surface-container-high, #ece6f0);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3, 0 8px 24px rgba(0, 0, 0, 0.2));
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .dialog-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--np-color-on-surface, #1d1b20);
  }
  .warning-text {
    color: var(--np-color-error, #b3261e);
  }
  .warning-body {
    margin: 0;
    text-align: center;
    font-size: 0.92rem;
    color: var(--np-color-on-surface-variant, #49454f);
    line-height: 1.5;
  }
  .dialog-footer-right {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .confirm-delete-wrapper {
    --np-button-container-color: var(--np-color-error, #b3261e);
    --np-button-label-text-color: var(--np-color-on-error, #ffffff);
  }
  .delete-warning-box {
    background: var(--np-color-surface-container-high);
    border-radius: 28px;
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .warning-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.25rem;
    color: var(--np-color-error);
  }
  .warning-desc-centered {
    margin: 0;
    text-align: center;
    font-size: 0.9rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.4;
  }
  .warning-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .danger-confirm-btn {
    padding: 8px 18px;
    border-radius: 20px;
    border: none;
    background: var(--np-color-error);
    color: var(--np-color-on-error);
    font-weight: 600;
    cursor: pointer;
  }
</style>
