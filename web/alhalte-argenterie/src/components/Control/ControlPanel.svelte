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
  import type { Character, ExtensionItem, QueuedLogMessage, Scene } from '../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../utils/assets.js';
  import type { ConnectionState } from '../../utils/connection.js';
  import type { AudioManager } from '../../utils/audio.js';
  import { ChunkLogManager } from '../../utils/log.js';
  import type { RoomSocket } from '../../utils/roomSocket.js';
  import type { SceneMovementEvent } from '../../utils/sceneGraph.js';
  import { ExtensionPipeline } from '../../wasm/runner.js';
  import type { RoomRole } from '../../utils/session.js';
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
    role = 'participant',
    defaultPermission = null,
    chunkManager = new ChunkLogManager(),
    onTabChange,
    onSelectScene,
    onSelectCharacter,
    onSendMessage,
    onUpdateScene,
    onCreateScene,
    onDeleteScene,
    onUpdateCharacter,
    onCreateCharacter,
    onDeleteCharacter,
    onTogglePiece,
    onMoveCharacter,
    onFocusCharacterScene,
    currentAccountId = null,
    onUpdateExtensions,
    onRegisterExtension,
    onExportZip,
    onCollapse,
    roomId,
    sending = false,
    queuedMessages = [],
    onManagePermissions,
    onSetDefaultPermission,
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
    audioManager = undefined,
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
    /**
     * Role of the caller (Issue #201). Every child derives its own
     * capabilities from it; `isAdmin` / `isViewer` are no longer passed around.
     */
    role?: RoomRole;
    /** Current default permission shown by the header selector (`null` = unknown). */
    defaultPermission?: RoomRole | null;
    chunkManager?: ChunkLogManager;
    onTabChange?: (tab: ActiveTab) => void;
    onSelectScene?: (sceneId: string) => void;
    onSelectCharacter?: (charId: string) => void;
    onSendMessage?: (text: string, characterId?: string) => void;
    onUpdateScene?: (scene: Scene) => void;
    onCreateScene?: (name: string, inheritJuice: boolean) => void;
    onDeleteScene?: (sceneId: string) => void;
    /**
     * Moves a character to an existing scene by ID (Issue #228). The character
     * tab has no free-text destination and never creates a scene.
     */
    onMoveCharacter?: (
      characterId: string,
      targetSceneId: string,
    ) => Promise<boolean | void> | boolean | void;
    /** Switches the caller's scene to the character's scene (視点を合わせる). */
    onFocusCharacterScene?: (sceneId: string) => void;
    /** Account ID used for the "自分のキャラクター" view and ownership gating. */
    currentAccountId?: string | null;
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
    /**
     * Forwarded to `LogTab`: `kind: 'message'` outbox entries for the active
     * room, rendered inline below the loaded messages (Issue #148).
     */
    queuedMessages?: QueuedLogMessage[];
    onManagePermissions?: () => void;
    /** Sends `SetDefaultPermission` for the header default permission selector (Issue #201). */
    onSetDefaultPermission?: (permission: RoomRole) => void | Promise<unknown>;
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
     * Forwarded to `SceneSettingsTab`: the per-App BGM audio manager
     * (Issue #164). `App` owns the instance so playback state and the settings
     * tab share one element per slot.
     */
    audioManager?: AudioManager;
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
    {role}
    {defaultPermission}
    {onCollapse}
    {onSelectScene}
    {onExportZip}
    onRenameScene={() => (activeTab = 'settings')}
    onDeleteScene={() => (activeTab = 'settings')}
    onManageMembers={() => (activeTab = 'settings')}
    {onManagePermissions}
    {onSetDefaultPermission}
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
        {role}
        {defaultPermission}
        {chunkManager}
        {roomSocket}
        {connectionState}
        {queuedMessages}
        {onCollapse}
        {onSelectScene}
        {onExportZip}
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
        {role}
        onNavigateToScene={handleNavigateFromGraph}
        onOpenSceneLog={handleOpenSceneLog}
        {onDeleteScene}
        {onRequestDeleteScene}
      />
    {:else if activeTab === 'settings'}
      <SceneSettingsTab
        {currentScene}
        {role}
        {onUpdateScene}
        {onCreateScene}
        {onDeleteScene}
        {onRequestDeleteScene}
        {onRequestDeleteSceneLogs}
        {onDeleteSceneLogs}
        {onRequestAssetPicker}
        {roomSocket}
        {audioManager}
        {roomKey}
        {sceneActionPending}
        {sceneActionError}
      />
    {:else if activeTab === 'characters'}
      <CharacterManagementTab
        {characters}
        {allScenes}
        {currentScene}
        {currentAccountId}
        {role}
        {sceneActionPending}
        {sceneActionError}
        {onUpdateCharacter}
        {onCreateCharacter}
        {onDeleteCharacter}
        {onTogglePiece}
        {onMoveCharacter}
        {onFocusCharacterScene}
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
        {role}
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
    {role}
    {sending}
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
