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
  import type { Character, ExtensionItem, LogRecord, Scene, ViewTransform, VisitedRoom } from '../types/index.js';
  import { saveRecentRoom, type RecentRoom } from '../utils/recentRooms.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../utils/assets.js';
  import { resolveRoomFromLocation } from '../utils/home.js';
  import { ChunkLogManager } from '../utils/log.js';
  import type { SceneMovementEvent } from '../utils/sceneGraph.js';
  import { createDefaultJuices, inheritJuices, moveMemberToScene } from '../utils/sceneSettings.js';
  import { applyThemeToDocument } from '../utils/theme.js';
  import {
    DEFAULT_OFFICIAL_EXTENSIONS,
    ExtensionPipeline,
    loadDefaultExtensions,
    WasmExtensionInstance,
  } from '../wasm/runner.js';
  import AssetPickerModal from './Control/AssetPickerModal.svelte';
  import ControlPanel from './Control/ControlPanel.svelte';
  import FieldPanel from './Field/FieldPanel.svelte';
  import HomeScreen from './Home/HomeScreen.svelte';
  import OfflineIndicator from './OfflineIndicator.svelte';
  import {
    connectionManager as defaultConnectionManager,
    ConnectionManager,
    type ConnectionState,
  } from '../utils/connection.js';
  import { RoomSocket, parseCharacter } from '../utils/roomSocket.js';

  let {
    initialRoom,
    initialScenes = [],
    initialCharacters = [],
    initialExtensions = [],
    initialMovements = [],
    isAdmin = false,
    connectionManager: customConnectionManager,
    connectionState,
    onReconnect,
    debugMode,
    debugAlwaysShow = false,
    roomSocket: customRoomSocket,
  }: {
    initialRoom?: VisitedRoom | null;
    initialScenes?: Scene[];
    initialCharacters?: Character[];
    initialExtensions?: ExtensionItem[];
    initialMovements?: SceneMovementEvent[];
    isAdmin?: boolean;
    connectionManager?: ConnectionManager;
    connectionState?: ConnectionState;
    onReconnect?: () => Promise<void> | void;
    debugMode?: boolean;
    debugAlwaysShow?: boolean;
    roomSocket?: RoomSocket;
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
    await activeConnectionManager.reconnect();
  }

  let currentRoom = $state<VisitedRoom | null>(
    initialRoom !== undefined ? initialRoom : resolveRoomFromLocation(),
  );
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
  let extensionFileInput: HTMLInputElement | null = $state(null);

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

  function handleDeleteSceneLogs(sceneId: string) {
    chunkManager.clearSceneRecords(sceneId);
    if (currentRoom?.id) {
      persistRoomLogs(currentRoom.id);
    }
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
        if (loaded.length > 0) {
          extensions = loaded;
        }
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

  let roomSocket = $state<RoomSocket | null>(customRoomSocket ?? null);

  $effect(() => {
    if (customRoomSocket) {
      roomSocket = customRoomSocket;
      return;
    }

    const roomId = currentRoom?.id || (currentRoom as any)?.roomId;
    if (roomId) {
      const socket = new RoomSocket({
        roomId,
        url: currentRoom?.url || (currentRoom as any)?.roomUrl,
        connectionManager: activeConnectionManager,
        chunkLogManager: chunkManager,
        initialSceneId: currentSceneId,
        autoConnect: true,
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

  $effect(() => {
    if (!roomSocket) return;
    roomSocket.setChunkLogManager(chunkManager);
    const unsubscribe = roomSocket.subscribe((frame) => {
      if (typeof frame !== 'object' || frame === null) return;
      if ('SceneList' in frame) {
        const sceneIds = frame.SceneList.scenes;
        const existingIds = new Set(scenes.map((s) => s.id));
        const newScenes = [...scenes];
        let changed = false;
        for (const sId of sceneIds) {
          if (!existingIds.has(sId)) {
            newScenes.push({
              id: sId,
              name: sId === '/' ? 'メインシーン' : sId,
              juices: createDefaultJuices(),
              viewTransform: { x: 0, y: 0, scale: 1 },
              members: [],
            });
            changed = true;
          }
        }
        if (changed) scenes = newScenes;
      }
      if ('CharacterList' in frame) {
        const charList: Character[] = [];
        if (frame.CharacterList?.characters) {
          for (const [id, raw] of Object.entries(frame.CharacterList.characters)) {
            charList.push(parseCharacter(id, raw));
          }
        }
        if (charList.length > 0) {
          characters = charList;
          if (!selectedCharacterId && characters.length > 0) {
            selectedCharacterId = characters[0].id;
          }
        }
      }
      if ('CharacterDeleted' in frame) {
        const idStr = String(frame.CharacterDeleted.character_id);
        characters = characters.filter((c) => c.id !== idStr);
        for (const sc of scenes) {
          sc.members = sc.members.filter((id) => id !== idStr);
        }
      }
      if ('StateUpdated' in frame) {
        const { scene_id, key, value } = frame.StateUpdated;
        const targetScene = scenes.find((s) => s.id === scene_id);
        if (targetScene) {
          if (key === 'std::bgm') {
            targetScene.juices.bgm1 = typeof value === 'string' ? value : undefined;
          } else if (key === 'std::b-img') {
            targetScene.juices.bgImage = typeof value === 'string' ? value : undefined;
          } else if (key === 'view::transform' && value && typeof value === 'object') {
            targetScene.viewTransform = { ...targetScene.viewTransform, ...value };
          }
        }
      }
      if ('StateDeleted' in frame) {
        const { scene_id, key } = frame.StateDeleted;
        const targetScene = scenes.find((s) => s.id === scene_id);
        if (targetScene) {
          if (key === 'std::bgm') {
            targetScene.juices.bgm1 = undefined;
          } else if (key === 'std::b-img') {
            targetScene.juices.bgImage = undefined;
          }
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
      roomSocket?.enterScene(sceneId);
    }
  }

  function handleCreateScene(name: string, inheritJuice: boolean) {
    const newId = `scene-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const newScene: Scene = {
      id: newId,
      name,
      juices: inheritJuice ? inheritJuices(currentScene.juices) : createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    scenes.push(newScene);
    currentSceneId = newId;
    roomSocket?.createScene(newId, inheritJuice);
  }

  function handleDeleteScene(sceneId: string) {
    if (scenes.length <= 1) return; // Keep at least one scene
    scenes = scenes.filter((s) => s.id !== sceneId);
    if (currentSceneId === sceneId) {
      currentSceneId = scenes[0].id;
    }
    roomSocket?.deleteScene(sceneId);
  }

  function handleMoveMember(characterId: string, targetSceneName: string) {
    const prevScene = scenes.find((s) => s.members.includes(characterId)) || currentScene;
    const res = moveMemberToScene(characterId, targetSceneName, currentScene, scenes);
    scenes = res.updatedScenes;
    currentSceneId = res.targetScene.id;

    // Record movement in movements list
    const char = characters.find((c) => c.id === characterId);
    movements.push({
      characterId,
      characterName: char?.name || 'Unknown',
      fromSceneId: prevScene.id,
      toSceneId: res.targetScene.id,
      timestamp: Math.floor(Date.now() / 1000),
    });

    const numericCharId = Number(characterId);
    if (!Number.isNaN(numericCharId)) {
      roomSocket?.moveCharacter(numericCharId, res.targetScene.id);
    }
    roomSocket?.updateState(res.targetScene.id, `character::${characterId}::scene`, res.targetScene.id);
  }

  function handleTogglePiece(characterId: string) {
    characters = characters.map((c) => {
      if (c.id === characterId) {
        const nextPlaced = !c.placedAsPiece;
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
    characters = characters.map((c) => {
      if (c.id === characterId) {
        return {
          ...c,
          piecePosition: { ...position },
        };
      }
      return c;
    });
    roomSocket?.updateState(currentScene.id, `piece::${characterId}::pos`, position);
  }

  function handleCreateCharacter(char: Character) {
    characters.push(char);
    if (!currentScene.members.includes(char.id)) {
      currentScene.members.push(char.id);
    }
    const numId = Number(char.id);
    roomSocket?.createCharacter(Number.isNaN(numId) ? Date.now() : numId, char);
  }

  function handleUpdateCharacter(char: Character) {
    characters = characters.map((c) => (c.id === char.id ? char : c));
    const numId = Number(char.id);
    roomSocket?.updateCharacter(Number.isNaN(numId) ? 0 : numId, char);
  }

  function handleDeleteCharacter(charId: string) {
    characters = characters.filter((c) => c.id !== charId);
    for (const sc of scenes) {
      sc.members = sc.members.filter((id) => id !== charId);
    }
    const numId = Number(charId);
    roomSocket?.deleteCharacter(Number.isNaN(numId) ? 0 : numId);
  }

  function handleTransformChange(id: string, t: ViewTransform) {
    const sc = scenes.find((s) => s.id === id);
    if (sc) sc.viewTransform = t;
    roomSocket?.updateState(id, 'view::transform', t);
  }

  function handleUploadExtension() {
    extensionFileInput?.click();
  }

  async function handleExtensionFileChange(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const instance = await WasmExtensionInstance.instantiate(bytes);
      const intro = instance.selfIntroduction();
      const id = `ext-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      extensionPipeline.registerInstance(id, instance);

      const newExt: ExtensionItem = {
        id,
        name: intro?.name || file.name.replace(/\.wasm$/i, ''),
        iconSvg: intro?.iconSvg,
        description: intro?.description,
        enabled: true,
        priority: extensions.length,
        wasmBytes: bytes,
      };
      extensions = [...extensions, newExt];
    } catch (err) {
      console.error('Failed to load uploaded extension wasm:', err);
    } finally {
      input.value = '';
    }
  }

  function handleSendMessage(text: string, characterId?: string) {
    const char = characters.find((c) => c.id === characterId);
    const speaker = char?.name || 'Anonymous';
    const record: LogRecord = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      sceneId: currentScene.id,
      content: text,
      speaker,
      speakerId: characterId,
      timestamp: Math.floor(Date.now() / 1000),
    };

    chunkManager.appendRecord(record);
    if (currentRoom?.id) {
      persistRoomLogs(currentRoom.id);
    }
    roomSocket?.appendLog(currentScene.id, record);

    if (extensions.some((ext) => ext.enabled && ext.wasmBytes)) {
      const initialBytes = new TextEncoder().encode(text);
      extensionPipeline.executePipeline(extensions, initialBytes).catch(() => {
        // Keep raw message displayed if extension pipeline fails
      });
    }
  }
</script>

<div class="alhalte-app-root" data-testid="alhalte-app">
  <input
    type="file"
    accept=".wasm,application/wasm"
    bind:this={extensionFileInput}
    onchange={handleExtensionFileChange}
    style="display: none;"
    data-testid="extension-file-input"
  />

  <!-- Server connection / offline detection indicator at frontmost top of viewport -->
  <OfflineIndicator
    connectionState={currentConnectionState}
    {debugMode}
    {debugAlwaysShow}
    onReconnect={handleReconnectFromIndicator}
  />

  {#if !currentRoom}
    <HomeScreen onEnterRoom={handleEnterRoom} onSelectRoom={handleEnterRoom} />
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
                <div class="side-sheet-toggle-wrapper">
                  <IconButton
                    variant="tonal"
                    size="xs"
                    class="side-sheet-toggle-btn"
                    data-testid="side-sheet-toggle-button"
                    onclick={(e) => {
                      e.stopPropagation();
                      isControlSheetOpen = false;
                    }}
                    aria-label="コントロールパネルを閉じる"
                  >
                    <Icon>chevron_right</Icon>
                  </IconButton>
                </div>
              </div>

              <!-- Main Control Panel -->
              <div class="side-sheet-content-wrapper">
                <ControlPanel
                  {currentScene}
                  allScenes={scenes}
                  {characters}
                  {extensions}
                  {movements}
                  {accessibleSceneIds}
                  {selectedCharacterId}
                  isAdmin={isAdmin || currentRoom.isAdmin}
                  {chunkManager}
                  onCollapse={() => (isControlSheetOpen = false)}
                  onSelectScene={handleSelectScene}
                  onSelectCharacter={(id) => (selectedCharacterId = id)}
                  onCreateScene={handleCreateScene}
                  onDeleteScene={handleDeleteScene}
                  onMoveMember={handleMoveMember}
                  onUpdateCharacter={handleUpdateCharacter}
                  onCreateCharacter={handleCreateCharacter}
                  onDeleteCharacter={handleDeleteCharacter}
                  onTogglePiece={handleTogglePiece}
                  onUpdateExtensions={(exts) => (extensions = exts)}
                  onUploadExtension={handleUploadExtension}
                  onSendMessage={handleSendMessage}
                  onRequestDeleteScene={handleRequestDeleteScene}
                  onRequestDeleteSceneLogs={handleRequestDeleteSceneLogs}
                  onDeleteSceneLogs={handleDeleteSceneLogs}
                  onRequestDeleteCharacter={handleRequestDeleteCharacter}
                  roomId={currentRoom.id || (currentRoom as any).roomId}
                  onSetDone={(done) => roomSocket?.setDone(done)}
                  onRequestAssetPicker={handleRequestAssetPicker}
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
                {movements}
                {accessibleSceneIds}
                {selectedCharacterId}
                isAdmin={isAdmin || currentRoom.isAdmin}
                {chunkManager}
                onCollapse={() => (isControlSheetOpen = false)}
                onSelectScene={handleSelectScene}
                onSelectCharacter={(id) => (selectedCharacterId = id)}
                onCreateScene={handleCreateScene}
                onDeleteScene={handleDeleteScene}
                onMoveMember={handleMoveMember}
                onUpdateCharacter={handleUpdateCharacter}
                onCreateCharacter={handleCreateCharacter}
                onDeleteCharacter={handleDeleteCharacter}
                onTogglePiece={handleTogglePiece}
                onUpdateExtensions={(exts) => (extensions = exts)}
                onUploadExtension={handleUploadExtension}
                onSendMessage={handleSendMessage}
                onRequestDeleteScene={handleRequestDeleteScene}
                onRequestDeleteSceneLogs={handleRequestDeleteSceneLogs}
                onDeleteSceneLogs={handleDeleteSceneLogs}
                onRequestDeleteCharacter={handleRequestDeleteCharacter}
                roomId={currentRoom.id || (currentRoom as any).roomId}
                onSetDone={(done) => roomSocket?.setDone(done)}
                onRequestAssetPicker={handleRequestAssetPicker}
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
                onclick={() => {
                  if (activeOverlay?.type === 'delete-scene') {
                    handleDeleteScene(activeOverlay.sceneId);
                    activeOverlay = null;
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
              onclick={() => {
                if (activeOverlay?.type === 'delete-scene-logs') {
                  handleDeleteSceneLogs(activeOverlay.sceneId);
                  activeOverlay = null;
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
                handleDeleteCharacter(activeOverlay.characterId);
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
        isAdmin={isAdmin || (currentRoom ? currentRoom.isAdmin : false)}
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
  .room-layout-container {
    width: 100%;
    height: 100%;
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
  }
  .landscape-control :global(.landscape-side-sheet) {
    min-width: 10vw !important;
    height: 100%;
    border-left: 1px solid var(--np-color-outline-variant);
    box-shadow: var(--np-elevation-2);
  }
  .landscape-control :global(.landscape-side-sheet.is-resizing) {
    transition: none !important;
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
  .side-sheet-toggle-wrapper {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
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
    z-index: 300;
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
    min-height: calc(100vh - 56px);
    display: flex;
    flex-direction: column;
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
    z-index: 1000;
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
