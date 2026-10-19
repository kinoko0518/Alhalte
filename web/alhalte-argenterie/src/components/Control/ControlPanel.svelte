<script lang="ts">
  /**
   * Control panel containing the active tab content and common footer.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通フッター`
   * - GitHub Issue #32: Material 3 semantic tokens & sheet collapse support across tabs
   */
  import { IconButton } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character, ExtensionItem, Scene } from '../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../utils/assets.js';
  import type { ConnectionState } from '../../utils/connection.js';
  import { ChunkLogManager } from '../../utils/log.js';
  import type { RoomSocket } from '../../utils/roomSocket.js';
  import type { SceneMovementEvent } from '../../utils/sceneGraph.js';
  import { ExtensionPipeline } from '../../wasm/runner.js';
  import CommonFooter, { type ActiveTab } from './CommonFooter.svelte';
  import CommonHeader from './CommonHeader.svelte';
  import CharacterManagementTab from './tabs/CharacterManagementTab.svelte';
  import ExtensionManagementTab from './tabs/ExtensionManagementTab.svelte';
  import LogTab from './tabs/LogTab.svelte';
  import SceneGraphTab from './tabs/SceneGraphTab.svelte';
  import SceneSettingsTab from './tabs/SceneSettingsTab.svelte';

  let {
    currentScene,
    allScenes = [],
    characters = [],
    extensions = [],
    extensionPipeline = new ExtensionPipeline(),
    movements = [],
    accessibleSceneIds = new Set<string>(),
    selectedCharacterId,
    isAdmin = false,
    chunkManager = new ChunkLogManager(),
    onTabChange,
    onSelectScene,
    onSelectCharacter,
    onSendMessage,
    onUpdateScene,
    onCreateScene,
    onDeleteScene,
    onMoveMember,
    onUpdateCharacter,
    onCreateCharacter,
    onDeleteCharacter,
    onTogglePiece,
    onUpdateExtensions,
    onRegisterExtension,
    onExportZip,
    onCollapse,
    roomId,
    isViewer = false,
    sending = false,
    pendingMessageCount = 0,
    onSetDone,
    onManagePermissions,
    onRequestDeleteScene,
    onRequestDeleteSceneLogs,
    onDeleteSceneLogs,
    onRequestDeleteCharacter,
    onRequestAssetPicker,
    onRequestEditCharacter,
    onRequestCreateCharacter,
    onRefreshCharacters,
    pendingCharacterCount,
    roomSocket = null,
    roomKey,
    connectionState,
    sceneActionPending = null,
    sceneActionError = null,
  }: {
    currentScene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    extensions?: ExtensionItem[];
    extensionPipeline?: ExtensionPipeline;
    movements?: SceneMovementEvent[];
    accessibleSceneIds?: Set<string>;
    selectedCharacterId?: string;
    isAdmin?: boolean;
    isViewer?: boolean;
    chunkManager?: ChunkLogManager;
    onTabChange?: (tab: ActiveTab) => void;
    onSelectScene?: (sceneId: string) => void;
    onSelectCharacter?: (charId: string) => void;
    onSendMessage?: (text: string, characterId?: string) => void;
    onUpdateScene?: (scene: Scene) => void;
    onCreateScene?: (name: string, inheritJuice: boolean) => void;
    onDeleteScene?: (sceneId: string) => void;
    onMoveMember?: (characterId: string, targetSceneName: string) => void;
    onUpdateCharacter?: (character: Character) => void;
    onCreateCharacter?: (character: Character) => void;
    onDeleteCharacter?: (characterId: string) => void;
    onTogglePiece?: (characterId: string) => void;
    onUpdateExtensions?: (extensions: ExtensionItem[]) => void;
    /** Registers downloaded / uploaded wasm with the pipeline (Issue #109 / #145). */
    onRegisterExtension?: (id: string, wasmBytes: Uint8Array) => void;
    onExportZip?: (type: 'full' | 'logs') => void;
    onCollapse?: () => void;
    roomId?: string;
    /** Forwarded to `CommonFooter`: a message send is waiting for its echo (Issue #109). */
    sending?: boolean;
    /** Forwarded to `CommonFooter`: messages waiting in the durable offline outbox (Issue #109). */
    pendingMessageCount?: number;
    onSetDone?: (done: boolean) => void;
    onManagePermissions?: () => void;
    onRequestDeleteScene?: (scene: { id: string; name: string }) => void;
    onRequestDeleteSceneLogs?: (scene: { id: string; name: string }) => void;
    onDeleteSceneLogs?: (sceneId: string) => void;
    onRequestDeleteCharacter?: (char: { id: string; name: string }) => void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    onRequestEditCharacter?: (char: Character) => void;
    /** Forwards a new-character draft to the hoisted root editor (Issue #109 / #144). */
    onRequestCreateCharacter?: (draft: Character) => void;
    /** Hands the authoritative `ListCharacters` result back to `App` (Issue #109 / #144). */
    onRefreshCharacters?: (characters: Character[]) => void;
    /** Forwards the `character` outbox count to the tab's queue notice (Issue #109 / #144). */
    pendingCharacterCount?: number;
    /**
     * Forwarded to `LogTab` (eager latest-chunk load, Issue #109 / #140) and to
     * `SceneSettingsTab` (acknowledged BGM state changes, Issue #109 / #142).
     */
    roomSocket?: RoomSocket | null;
    /**
     * Forwarded to `ExtensionManagementTab`: 32-byte room key for the wasm
     * cellar transfer (Issue #109 / sub-issue #145).
     */
    roomKey?: Uint8Array | null;
    /** Forwarded to `LogTab`: retries a queued log load when it becomes `connected`. */
    connectionState?: ConnectionState;
    /**
     * Forwarded to `SceneSettingsTab`: the scene action whose control shows a
     * wavy `CircularProgress` while the server acknowledges it
     * (Issue #109 / sub-issue #143).
     */
    sceneActionPending?: string | null;
    /** Forwarded to `SceneSettingsTab`: scene-action failure text (Issue #109 / #143). */
    sceneActionError?: string | null;
  } = $props();

  let activeTab = $state<ActiveTab>('log');

  function handleTabChange(tab: ActiveTab) {
    activeTab = tab;
    onTabChange?.(tab);
  }

  let sceneNamesMap = $derived.by(() => {
    const map = new Map<string, string>();
    for (const sc of allScenes) {
      map.set(sc.id, sc.name);
    }
    return map;
  });

  function handleNavigateFromGraph(sceneId: string) {
    onSelectScene?.(sceneId);
  }

  function handleOpenSceneLog(sceneId: string) {
    onSelectScene?.(sceneId);
    activeTab = 'log';
  }
</script>

<div class="control-panel-container" data-testid="control-panel">
  <!-- Unified Common Header across all tabs (Issue #89) -->
  <CommonHeader
    scene={currentScene}
    {allScenes}
    {characters}
    {roomId}
    {isAdmin}
    {onCollapse}
    {onSelectScene}
    {onExportZip}
    onRenameScene={() => (activeTab = 'settings')}
    onDeleteScene={() => (activeTab = 'settings')}
    onManageMembers={() => (activeTab = 'settings')}
    {onManagePermissions}
    {onSetDone}
  />

  <!-- Active Tab Body -->
  <div class="tab-content-area" data-testid="tab-content-area">
    {#if activeTab === 'log'}
      <LogTab
        scene={currentScene}
        {allScenes}
        {characters}
        {extensions}
        {extensionPipeline}
        {roomId}
        {isAdmin}
        {chunkManager}
        {roomSocket}
        {connectionState}
        {onCollapse}
        {onSelectScene}
        {onExportZip}
        {onSetDone}
        {onRequestAssetPicker}
        hideHeader={true}
        onRenameScene={() => (activeTab = 'settings')}
        onDeleteScene={() => (activeTab = 'settings')}
        onManageMembers={() => (activeTab = 'settings')}
      />
    {:else if activeTab === 'graph'}
      <SceneGraphTab
        {movements}
        sceneNames={sceneNamesMap}
        {accessibleSceneIds}
        {isAdmin}
        {isViewer}
        onNavigateToScene={handleNavigateFromGraph}
        onOpenSceneLog={handleOpenSceneLog}
        {onDeleteScene}
        {onRequestDeleteScene}
      />
    {:else if activeTab === 'settings'}
      <SceneSettingsTab
        {currentScene}
        {allScenes}
        {characters}
        {isAdmin}
        {isViewer}
        {onUpdateScene}
        {onSelectScene}
        {onCreateScene}
        {onDeleteScene}
        {onMoveMember}
        {onRequestDeleteScene}
        {onRequestDeleteSceneLogs}
        {onDeleteSceneLogs}
        {onRequestAssetPicker}
        {roomSocket}
        {roomKey}
        {sceneActionPending}
        {sceneActionError}
      />
    {:else if activeTab === 'characters'}
      <CharacterManagementTab
        {characters}
        {isAdmin}
        {isViewer}
        {onUpdateCharacter}
        {onCreateCharacter}
        {onDeleteCharacter}
        {onTogglePiece}
        {onRequestDeleteCharacter}
        {onRequestAssetPicker}
        {onRequestEditCharacter}
        {onRequestCreateCharacter}
        {onRefreshCharacters}
        {pendingCharacterCount}
        {roomSocket}
        {roomId}
      />
    {:else if activeTab === 'extensions'}
      <ExtensionManagementTab
        {extensions}
        {isAdmin}
        {isViewer}
        {roomSocket}
        {connectionState}
        {roomKey}
        {onUpdateExtensions}
        {onRegisterExtension}
      />
    {/if}
  </div>

  <!-- Common Footer -->
  <CommonFooter
    {activeTab}
    {characters}
    {extensions}
    {extensionPipeline}
    {selectedCharacterId}
    {isViewer}
    {sending}
    {pendingMessageCount}
    onTabChange={handleTabChange}
    {onSelectCharacter}
    {onSendMessage}
  />
</div>

<style>
  .control-panel-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--np-color-surface);
    color: var(--np-color-on-surface);
    border-left: 1px solid var(--np-color-outline-variant);
  }
  .tab-content-area {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
</style>
