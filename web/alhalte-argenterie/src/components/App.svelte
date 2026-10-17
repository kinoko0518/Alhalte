<script lang="ts">
  /**
   * Main App component for @alhalte/argenterie.
   *
   * Layout adheres to Material 3 Expressive:
   * - Landscape: Left side field panel, Right side control panel.
   * - Portrait: Full-screen field panel with bottom sheet control panel.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie`
   */
  import { Sheet } from 'noph-ui';
  import type { Character, ExtensionItem, Scene, VisitedRoom } from '../types/index.js';
  import { ChunkLogManager } from '../utils/log.js';
  import type { SceneMovementEvent } from '../utils/sceneGraph.js';
  import { createDefaultJuices, inheritJuices, moveMemberToScene } from '../utils/sceneSettings.js';
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

  let currentRoom = $state<VisitedRoom | null>(initialRoom ?? null);
  let isLandscape = $state(
    typeof window !== 'undefined' ? window.innerWidth >= window.innerHeight : true,
  );
  let isBottomSheetOpen = $state(true);

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
  let extensions = $state<ExtensionItem[]>(initialExtensions);
  let movements = $state<SceneMovementEvent[]>(initialMovements);
  let selectedCharacterId = $state<string | undefined>(characters[0]?.id);

  let accessibleSceneIds = $derived.by(() => {
    return new Set(scenes.map((s) => s.id));
  });

  const chunkManager = new ChunkLogManager();

  // Orientation listener
  function updateOrientation() {
    if (typeof window !== 'undefined') {
      isLandscape = window.innerWidth >= window.innerHeight;
    }
  }

  $effect(() => {
    updateOrientation();
    if (typeof window !== 'undefined') {
      window.addEventListener('resize', updateOrientation);
      return () => window.removeEventListener('resize', updateOrientation);
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
    const res = moveMemberToScene(characterId, targetSceneName, currentScene, scenes);
    scenes = res.updatedScenes;
    currentSceneId = res.targetScene.id;

    // Record movement in movements list
    const char = characters.find((c) => c.id === characterId);
    movements.push({
      characterId,
      characterName: char?.name || 'Unknown',
      fromSceneId: currentScene.id,
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

  function handleSendMessage(text: string, characterId?: string) {
    const speaker = characters.find((c) => c.id === characterId)?.name || 'Anonymous';
    // Append to scene logs
    const record = {
      id: `msg-${Date.now()}`,
      sceneId: currentScene.id,
      content: text,
      speaker,
      speakerId: characterId,
      timestamp: Math.floor(Date.now() / 1000),
    };
    // If scene chunks registered, we can append; also chunk manager gets loaded record
  }
</script>

<div class="alhalte-app-root" data-testid="alhalte-app">
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
          左側にスクリーンパネルや駒などを置くフィールドパネル、右側にコントロールパネル。
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
          />
        </div>

        <div class="control-panel-wrapper landscape-control" data-testid="landscape-control-wrapper">
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
            {handleTogglePiece}
            onUpdateExtensions={(exts) => (extensions = exts)}
            onSendMessage={handleSendMessage}
          />
        </div>
      {:else}
        <!-- Portrait layout:
          全体にフィールド、その上にボトムシートでコントロールパネル。
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
          />
        </div>

        <!-- Bottom Sheet Control Panel -->
        <div class="bottom-sheet-container" data-testid="portrait-bottom-sheet">
          <button
            type="button"
            class="sheet-drag-handle"
            onclick={() => (isBottomSheetOpen = !isBottomSheetOpen)}
            aria-label="シートの開閉"
          >
            <div class="handle-bar"></div>
          </button>
          {#if isBottomSheetOpen}
            <div class="sheet-content">
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
                {handleTogglePiece}
                onUpdateExtensions={(exts) => (extensions = exts)}
                onSendMessage={handleSendMessage}
              />
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: Roboto, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }
  .alhalte-app-root {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    position: relative;
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
    width: 440px;
    max-width: 45vw;
    height: 100%;
    position: relative;
    box-shadow: -4px 0 16px rgba(0, 0, 0, 0.15);
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
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--md-sys-color-surface-container, #f3edf7);
    border-top-left-radius: 28px;
    border-top-right-radius: 28px;
    box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.25);
    z-index: 500;
  }
  .sheet-drag-handle {
    width: 100%;
    padding: 10px 0 6px;
    display: flex;
    justify-content: center;
    cursor: pointer;
  }
  .handle-bar {
    width: 48px;
    height: 5px;
    background: var(--md-sys-color-outline, #79747e);
    border-radius: 3px;
  }
  .sheet-content {
    flex: 1;
    height: 60vh;
    overflow: hidden;
  }
</style>
