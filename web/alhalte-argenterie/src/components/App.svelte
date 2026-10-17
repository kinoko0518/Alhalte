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
  import type { Character, ExtensionItem, LogRecord, Scene, VisitedRoom } from '../types/index.js';
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

  let {
    initialRoom,
    initialScenes = [],
    initialCharacters = [],
    initialExtensions = [],
    initialMovements = [],
    isAdmin = false,
  }: {
    initialRoom?: VisitedRoom | null;
    initialScenes?: Scene[];
    initialCharacters?: Character[];
    initialExtensions?: ExtensionItem[];
    initialMovements?: SceneMovementEvent[];
    isAdmin?: boolean;
  } = $props();

  let currentRoom = $state<VisitedRoom | null>(
    initialRoom !== undefined ? initialRoom : resolveRoomFromLocation(),
  );
  let isLandscape = $state(
    typeof window !== 'undefined' ? window.innerWidth >= window.innerHeight : true,
  );
  let isControlSheetOpen = $state(true);

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
    }
  });

  function handleEnterRoom(room: VisitedRoom) {
    currentRoom = room;
  }

  function handleSelectScene(sceneId: string) {
    if (scenes.some((s) => s.id === sceneId)) {
      currentSceneId = sceneId;
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
  }

  function handleDeleteScene(sceneId: string) {
    if (scenes.length <= 1) return; // Keep at least one scene
    scenes = scenes.filter((s) => s.id !== sceneId);
    if (currentSceneId === sceneId) {
      currentSceneId = scenes[0].id;
    }
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
  }

  function handleTogglePiece(characterId: string) {
    characters = characters.map((c) => {
      if (c.id === characterId) {
        return {
          ...c,
          placedAsPiece: !c.placedAsPiece,
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
  {#if !currentRoom}
    <HomeScreen onEnterRoom={handleEnterRoom} />
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
            onTransformChange={(id, t) => {
              const sc = scenes.find((s) => s.id === id);
              if (sc) sc.viewTransform = t;
            }}
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
            class="landscape-side-sheet"
            --np-sheet-size="440px"
            data-testid="landscape-side-sheet"
          >
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
              onUpdateCharacter={(char) => {
                characters = characters.map((c) => (c.id === char.id ? char : c));
              }}
              onCreateCharacter={(char) => {
                characters.push(char);
                if (!currentScene.members.includes(char.id)) {
                  currentScene.members.push(char.id);
                }
              }}
              onDeleteCharacter={(charId) => {
                characters = characters.filter((c) => c.id !== charId);
                for (const sc of scenes) {
                  sc.members = sc.members.filter((id) => id !== charId);
                }
              }}
              onTogglePiece={handleTogglePiece}
              onUpdateExtensions={(exts) => (extensions = exts)}
              onUploadExtension={handleUploadExtension}
              onSendMessage={handleSendMessage}
              onRequestDeleteScene={handleRequestDeleteScene}
              onRequestDeleteCharacter={handleRequestDeleteCharacter}
              onRequestAssetPicker={handleRequestAssetPicker}
            />
          </Sheet>

          {#if !isControlSheetOpen}
            <div class="reopen-sheet-floating landscape-reopen">
              <IconButton
                variant="tonal"
                onclick={() => (isControlSheetOpen = true)}
                data-testid="reopen-sheet-button"
                aria-label="コントロールパネルを開く"
              >
                <Icon>left_panel_open</Icon>
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
            onTransformChange={(id, t) => {
              const sc = scenes.find((s) => s.id === id);
              if (sc) sc.viewTransform = t;
            }}
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
            placement="bottom"
            modal={false}
            handle={true}
            handleLabel="シートの開閉"
            class="portrait-bottom-sheet"
            data-testid="portrait-sheet-dialog"
          >
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
              onUpdateCharacter={(char) => {
                characters = characters.map((c) => (c.id === char.id ? char : c));
              }}
              onCreateCharacter={(char) => {
                characters.push(char);
                if (!currentScene.members.includes(char.id)) {
                  currentScene.members.push(char.id);
                }
              }}
              onDeleteCharacter={(charId) => {
                characters = characters.filter((c) => c.id !== charId);
                for (const sc of scenes) {
                  sc.members = sc.members.filter((id) => id !== charId);
                }
              }}
              onTogglePiece={handleTogglePiece}
              onUpdateExtensions={(exts) => (extensions = exts)}
              onUploadExtension={handleUploadExtension}
              onSendMessage={handleSendMessage}
              onRequestDeleteScene={handleRequestDeleteScene}
              onRequestDeleteCharacter={handleRequestDeleteCharacter}
              onRequestAssetPicker={handleRequestAssetPicker}
            />
          </Sheet>

          {#if !isControlSheetOpen}
            <div class="reopen-sheet-floating portrait-reopen">
              <IconButton
                variant="tonal"
                onclick={() => (isControlSheetOpen = true)}
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
  }
  .room-layout-container {
    width: 100%;
    height: 100%;
    display: flex;
    position: relative;
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
    --np-sheet-size: 440px;
    max-width: 45vw;
    height: 100%;
    border-left: 1px solid var(--np-color-outline-variant);
    box-shadow: var(--np-elevation-2);
  }
  .landscape-control :global(.landscape-side-sheet .np-sheet-content) {
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    height: 100%;
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
    --np-sheet-size: 65dvh;
    --np-sheet-max-width: 100%;
    height: 65dvh;
    box-shadow: var(--np-elevation-3);
  }
  .bottom-sheet-container :global(.portrait-bottom-sheet .np-sheet-content) {
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    height: 100%;
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
