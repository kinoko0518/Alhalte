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
  import { ChunkLogManager } from '../../utils/log.js';
  import type { SceneMovementEvent } from '../../utils/sceneGraph.js';
  import CommonFooter, { type ActiveTab } from './CommonFooter.svelte';
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
    movements = [],
    accessibleSceneIds = new Set<string>(),
    selectedCharacterId,
    isAdmin = true,
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
    onUploadExtension,
    onExportZip,
    onCollapse,
  }: {
    currentScene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    extensions?: ExtensionItem[];
    movements?: SceneMovementEvent[];
    accessibleSceneIds?: Set<string>;
    selectedCharacterId?: string;
    isAdmin?: boolean;
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
    onUploadExtension?: () => void;
    onExportZip?: () => void;
    onCollapse?: () => void;
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
  {#if activeTab !== 'log' && onCollapse}
    <div class="sheet-collapse-strip" data-testid="sheet-collapse-strip">
      <IconButton
        variant="text"
        size="xs"
        onclick={onCollapse}
        data-testid="sheet-collapse-button"
        aria-label="コントロールパネルをしまう"
      >
        <Icon>right_panel_close</Icon>
      </IconButton>
    </div>
  {/if}

  <!-- Active Tab Body -->
  <div class="tab-content-area" data-testid="tab-content-area">
    {#if activeTab === 'log'}
      <LogTab
        scene={currentScene}
        {isAdmin}
        {chunkManager}
        {onCollapse}
        {onExportZip}
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
        onNavigateToScene={handleNavigateFromGraph}
        onOpenSceneLog={handleOpenSceneLog}
        {onDeleteScene}
      />
    {:else if activeTab === 'settings'}
      <SceneSettingsTab
        {currentScene}
        {allScenes}
        {characters}
        {isAdmin}
        {onUpdateScene}
        {onSelectScene}
        {onCreateScene}
        {onDeleteScene}
        {onMoveMember}
      />
    {:else if activeTab === 'characters'}
      <CharacterManagementTab
        {characters}
        {onUpdateCharacter}
        {onCreateCharacter}
        {onDeleteCharacter}
        {onTogglePiece}
      />
    {:else if activeTab === 'extensions'}
      <ExtensionManagementTab
        {extensions}
        {isAdmin}
        {onUpdateExtensions}
        {onUploadExtension}
      />
    {/if}
  </div>

  <!-- Common Footer -->
  <CommonFooter
    {activeTab}
    {characters}
    {selectedCharacterId}
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
  .sheet-collapse-strip {
    display: flex;
    align-items: center;
    padding: 4px 8px;
    background: var(--np-color-surface-container);
    border-bottom: 1px solid var(--np-color-outline-variant);
  }
  .tab-content-area {
    flex: 1;
    overflow: hidden;
    position: relative;
  }
</style>
