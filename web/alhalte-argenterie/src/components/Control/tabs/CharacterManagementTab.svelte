<script lang="ts">
  /**
   * Character Management Tab & Character Edit Panel.
   *
   * The list is an M3 Expressive segmented list with a segmented button that
   * switches between the current scene, the caller's own characters and (for an
   * admin) every character. The available row actions depend on the role, the
   * view and `std::owner_account`; that matrix lives in
   * `utils/characterActions.ts` so the component contains no role branching
   * beyond rendering the result.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### キャラクター同期`
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
   * - GitHub Issue #228: キャラクター一覧画面に関する提言
   */
  import {
    Button,
    CircularProgress,
    IconButton,
    List,
    ListItem,
    LoadingIndicator,
    Menu,
    MenuItem,
    SegmentedButton,
  } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character, Scene } from '../../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../../utils/assets.js';
  import {
    availableCharacterViews,
    CHARACTER_VIEW_LABELS,
    isCharacterOwnedBy,
    visibleRowActions,
    type CharacterRowAction,
    type CharacterView,
  } from '../../../utils/characterActions.js';
  import { truncateDescription } from '../../../utils/character.js';
  import { roomRoleCapabilities, type RoomRole } from '../../../utils/session.js';
  import type { RoomSocket } from '../../../utils/roomSocket.js';
  import {
    applyMenuPosition,
    getMenuPositionStyle,
  } from '../../../utils/menuPosition.js';
  import OfflineQueueNotice from '../../OfflineQueueNotice.svelte';
  import CharacterEditModal from '../CharacterEditModal.svelte';
  import CharacterPieceToggle from '../CharacterPieceToggle.svelte';

  let {
    characters = [],
    allScenes = [],
    currentScene = null,
    currentAccountId = null,
    role = 'participant',
    roomSocket = null,
    roomId,
    pendingCharacterCount,
    sceneActionPending = null,
    sceneActionError = null,
    onUpdateCharacter,
    onCreateCharacter,
    onDeleteCharacter,
    onTogglePiece,
    onRequestDeleteCharacter,
    onRequestAssetPicker,
    onRequestEditCharacter,
    onRequestCreateCharacter,
    onRefreshCharacters,
    onFocusCharacterScene,
    onMoveCharacter,
  }: {
    characters?: Character[];
    /** Every scene of the room: used for scene labels and the "別シーン" menu. */
    allScenes?: Scene[];
    /**
     * Scene the caller is currently looking at. The default `scene` view lists
     * its members only. When omitted (standalone usage) every character is
     * treated as being in the scene.
     */
    currentScene?: Scene | null;
    /**
     * Account ID of the caller (`App` passes `getSavedAccountId()`). A character
     * is "mine" only when `std::owner_account` matches this exact decimal value.
     */
    currentAccountId?: string | null;
    /**
     * Role of the caller (Issue #201). `viewer` and `denied` cannot create,
     * edit, delete or place characters; `participant` may manage their own.
     */
    role?: RoomRole;
    /**
     * Socket used to query the authoritative character list when the tab is
     * shown (Issue #109 / sub-issue #144). Without it the tab renders the
     * cached `characters` prop only (standalone usage / tests).
     */
    roomSocket?: RoomSocket | null;
    /** Room the tab belongs to (informational; the outbox is keyed in `App`). */
    roomId?: string;
    /** Entries waiting in the durable outbox, forwarded to the queue notice. */
    pendingCharacterCount?: number;
    /** Scene action in flight in `App` (`move:<characterId>`); drives the spinner. */
    sceneActionPending?: string | null;
    /** Failure text owned by `App` (scene move failures); rendered with `localError`. */
    sceneActionError?: string | null;
    onUpdateCharacter?: (character: Character) => void;
    onCreateCharacter?: (character: Character) => void;
    onDeleteCharacter?: (characterId: string) => void;
    onTogglePiece?: (characterId: string) => void;
    onRequestDeleteCharacter?: (char: { id: string; name: string }) => void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    onRequestEditCharacter?: (char: Character) => void;
    /**
     * Requests the hoisted editor for a new draft. When provided, `App` owns the
     * modal and no character is created until the server acknowledges the save.
     */
    onRequestCreateCharacter?: (draft: Character) => void;
    /** Receives the authoritative list returned by `ListCharacters`. */
    onRefreshCharacters?: (characters: Character[]) => void;
    /**
     * Switches the caller's current scene to the character's scene
     * (「視点を合わせる」). `App` wires this to `handleSelectScene`.
     */
    onFocusCharacterScene?: (sceneId: string) => void;
    /**
     * Moves a character to an existing scene by ID (「別シーンに配置」). `App`
     * resolves the display name and follows the `MoveCharacter` ack path; this
     * tab never creates a scene.
     */
    onMoveCharacter?: (
      characterId: string,
      targetSceneId: string,
    ) => Promise<boolean | void> | boolean | void;
  } = $props();

  const CHARACTER_ACTION_ERROR_MESSAGE = 'サーバーと通信できませんでした';

  let isEditOpen = $state(false);
  let editingCharacter = $state<Character>({
    id: '',
    name: '',
    initiative: 0,
    description: '',
    reference: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    isNarrative: false,
  });
  let isDeleteConfirmOpen = $state(false);
  let characterToDelete = $state<Character | null>(null);

  // ---------------------------------------------------------------------------
  // Segmented views (Issue #228)
  // ---------------------------------------------------------------------------
  let views = $derived(availableCharacterViews(role));
  let view = $state<CharacterView>('scene');

  // A role change can remove the selected view (e.g. an admin demoted to
  // participant while `all` is selected); fall back to the always-present scene.
  $effect(() => {
    const available = availableCharacterViews(role);
    if (!available.includes(view)) view = 'scene';
  });

  /** The view actually rendered; guards a role change in the same tick. */
  let activeView = $derived<CharacterView>(views.includes(view) ? view : 'scene');

  let viewOptions = $derived(
    views.map((value) => ({
      value,
      label: CHARACTER_VIEW_LABELS[value],
      selected: view === value,
    })),
  );

  let capabilities = $derived(roomRoleCapabilities(role));

  /**
   * Lifecycle of the tab-open `ListCharacters` round trip
   * (Issue #109 / sub-issue #144):
   *
   * - `idle`: no socket (standalone) or the tab has not queried yet.
   * - `loading`: `ListCharacters` is in flight; the contained loading indicator
   *   floats over the (possibly cached) list.
   * - `loaded`: the authoritative list arrived and was handed to `App`.
   * - `queued`: the query timed out / failed; keep the cached list and show the
   *   wavy `OfflineQueueNotice kind="character"`.
   */
  type CharacterLoadState = 'idle' | 'loading' | 'loaded' | 'queued';

  let loadState = $state<CharacterLoadState>('idle');
  /** Socket the current load state belongs to; a new socket triggers a re-query. */
  let queriedSocket: RoomSocket | null = null;

  let isLoading = $derived(loadState === 'loading');
  /**
   * Queue notice: either the tab-open query failed, or `App` reports queued
   * character operations (a save timed out / was performed offline). Keeping
   * both sources in one derived flag means a queued save flips the tab into the
   * queueing state without an extra prop.
   */
  let isQueueing = $derived(loadState === 'queued' || (pendingCharacterCount ?? 0) > 0);

  $effect(() => {
    const socket = roomSocket;
    if (!socket) return;
    if (queriedSocket === socket) return;
    queriedSocket = socket;
    void queryCharacters(socket);
  });

  async function queryCharacters(socket: RoomSocket): Promise<void> {
    const isStale = () => queriedSocket !== socket;
    loadState = 'loading';
    try {
      const list = await socket.queryListCharacters();
      if (isStale()) return;
      // Replacing (not merging) keeps the tab free of duplicate records: the
      // server list is authoritative, including when it is empty.
      onRefreshCharacters?.(list);
      loadState = 'loaded';
    } catch {
      if (isStale()) return;
      // Timeout / connection failure: the cached `characters` prop stays
      // visible and the wavy notice explains that the list is stale.
      loadState = 'queued';
    }
  }

  // ---------------------------------------------------------------------------
  // Scene membership helpers (Issue #228)
  // ---------------------------------------------------------------------------
  /** Scene the character is currently a member of, if known. */
  function characterScene(characterId: string): Scene | undefined {
    return (
      allScenes.find((scene) => scene.members.includes(characterId)) ??
      (currentScene?.members.includes(characterId) ? currentScene : undefined)
    );
  }

  /**
   * True when the character is in the caller's current scene. Without a
   * `currentScene` (standalone usage) every character is treated as present, so
   * no 「視点を合わせる」 is offered.
   */
  function isInCurrentScene(characterId: string): boolean {
    if (!currentScene) return true;
    return currentScene.members.includes(characterId);
  }

  /** Scene name shown under the name outside the scene view (`未所属` if none). */
  function characterSceneLabel(characterId: string): string {
    return characterScene(characterId)?.name ?? '未所属';
  }

  let visibleCharacters = $derived.by(() => {
    if (activeView === 'scene') {
      if (!currentScene) return characters;
      return characters.filter((character) => currentScene.members.includes(character.id));
    }
    if (activeView === 'mine') {
      return characters.filter((character) =>
        isCharacterOwnedBy(character, currentAccountId),
      );
    }
    return characters;
  });

  function rowActions(character: Character): CharacterRowAction[] {
    return visibleRowActions({
      role,
      view: activeView,
      character,
      currentAccountId,
      inCurrentScene: isInCurrentScene(character.id),
      hasCharacterScene: characterScene(character.id) !== undefined,
    });
  }

  // ---------------------------------------------------------------------------
  // Menus (move / more), one open at a time and closed on outside click
  // ---------------------------------------------------------------------------
  type CharacterMenuKind = 'move' | 'more';
  let openMenu = $state<{ kind: CharacterMenuKind; characterId: string } | null>(null);
  let menuAnchors = $state<Record<string, HTMLElement | undefined>>({});
  let menuElements = $state<Record<string, HTMLElement | undefined>>({});

  function menuKey(kind: CharacterMenuKind, characterId: string): string {
    return `${kind}-${characterId}`;
  }

  function isMenuOpen(kind: CharacterMenuKind, characterId: string): boolean {
    return openMenu?.kind === kind && openMenu.characterId === characterId;
  }

  function toggleMenu(kind: CharacterMenuKind, characterId: string): void {
    openMenu = isMenuOpen(kind, characterId) ? null : { kind, characterId };
  }

  function closeMenu(): void {
    openMenu = null;
  }

  function handleWindowClick(event: MouseEvent): void {
    const active = openMenu;
    if (!active) return;
    const target = event.target;
    if (target instanceof Node) {
      const key = menuKey(active.kind, active.characterId);
      if (menuAnchors[key]?.contains(target) || menuElements[key]?.contains(target)) return;
    }
    openMenu = null;
  }

  $effect(() => {
    const active = openMenu;
    if (!active) return;
    const key = menuKey(active.kind, active.characterId);
    if (menuElements[key] && menuAnchors[key]) {
      return applyMenuPosition(menuElements[key], menuAnchors[key], {
        placement: 'bottom-end',
        anchorName: `--character-menu-anchor-${key}`,
      });
    }
  });

  /** Existing scenes a character can be moved to (never its current scene). */
  function moveTargetScenes(characterId: string): Scene[] {
    const currentSceneId = characterScene(characterId)?.id;
    return allScenes.filter((scene) => scene.id !== currentSceneId);
  }

  // ---------------------------------------------------------------------------
  // Row actions (Issue #228)
  // ---------------------------------------------------------------------------
  /** This tab's own pending action, so standalone usage shows the spinner too. */
  let localPendingAction = $state<string | null>(null);
  /** Failure text owned by this tab (a callback returned `false` / threw). */
  let localActionError = $state<string | null>(null);

  let effectiveActionError = $derived(sceneActionError || localActionError);
  let isAnyActionPending = $derived(localPendingAction !== null);

  function isMovePending(characterId: string): boolean {
    return (
      localPendingAction === `move:${characterId}` ||
      sceneActionPending === `move:${characterId}`
    );
  }

  async function runAction(
    actionId: string,
    action: (() => Promise<boolean | void> | boolean | void) | undefined,
  ): Promise<boolean> {
    if (isAnyActionPending || !action) return false;
    localActionError = null;
    localPendingAction = actionId;
    try {
      const result = await action();
      if (result === false) {
        localActionError = CHARACTER_ACTION_ERROR_MESSAGE;
        return false;
      }
      return true;
    } catch (error) {
      console.warn(`[CharacterManagementTab] Character action "${actionId}" failed:`, error);
      localActionError = CHARACTER_ACTION_ERROR_MESSAGE;
      return false;
    } finally {
      localPendingAction = null;
    }
  }

  async function handleMoveCharacter(character: Character, targetSceneId: string): Promise<void> {
    closeMenu();
    await runAction(`move:${character.id}`, () =>
      onMoveCharacter?.(character.id, targetSceneId),
    );
  }

  function handleFocusCharacter(character: Character): void {
    const scene = characterScene(character.id);
    if (!scene) return;
    try {
      onFocusCharacterScene?.(scene.id);
    } catch (error) {
      console.warn('[CharacterManagementTab] Failed to focus the character scene:', error);
      localActionError = CHARACTER_ACTION_ERROR_MESSAGE;
    }
  }

  // ---------------------------------------------------------------------------
  // Edit / create / delete (unchanged behaviour)
  // ---------------------------------------------------------------------------
  function openEdit(char: Character) {
    if (!capabilities.canManageCharacters) return;
    if (onRequestEditCharacter) {
      onRequestEditCharacter(char);
    } else {
      editingCharacter = JSON.parse(JSON.stringify(char));
      isEditOpen = true;
    }
  }

  function openDeleteConfirm(char: Character) {
    closeMenu();
    if (!capabilities.canManageCharacters) return;
    if (onRequestDeleteCharacter) {
      onRequestDeleteCharacter({ id: char.id, name: char.name });
    } else {
      characterToDelete = char;
      isDeleteConfirmOpen = true;
    }
  }

  function confirmDelete() {
    if (characterToDelete) {
      onDeleteCharacter?.(characterToDelete.id);
    }
    isDeleteConfirmOpen = false;
    characterToDelete = null;
  }

  function handleCreateNewChar() {
    if (!capabilities.canManageCharacters) return;
    const newChar: Character = {
      id: `char-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      name: '新規キャラクター',
      initiative: 0,
      description: '',
      reference: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      isNarrative: false,
      placedAsPiece: false,
    };
    if (onRequestCreateCharacter) {
      // Hoisted editor: `App` opens the root modal and does not create anything
      // until the server acknowledges the save (Issue #109 / #144).
      onRequestCreateCharacter(newChar);
      return;
    }
    // Standalone fallback (tests / tab without `App`): create locally and edit
    // in the tab-owned panel.
    onCreateCharacter?.(newChar);
    openEdit(newChar);
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="character-tab-container" data-testid="character-management-tab">
  <div class="tab-header">
    <h3 class="tab-title">キャラクター一覧</h3>
    <IconButton
      variant="filled"
      onclick={handleCreateNewChar}
      disabled={!capabilities.canManageCharacters}
      aria-label="新規作成"
      title="新規作成"
      data-testid="create-character-btn"
    >
      <Icon>person_add</Icon>
    </IconButton>
  </div>

  <!-- Segmented button: viewers only have the scene view, so it is hidden. -->
  {#if views.length > 1}
    <div class="character-view-tabs" data-testid="character-view-tabs">
      <SegmentedButton
        name="character-view"
        options={viewOptions}
        bind:group={view}
        aria-label="キャラクターの表示"
        data-testid="character-view-segmented"
      />
    </div>
  {/if}

  <!-- Queue state: the tab-open query timed out, or a save was queued in the
       durable outbox (Issue #109 / #144). The cached list stays visible. -->
  {#if isQueueing}
    <div class="character-queue-notice" data-testid="character-queue-notice">
      <OfflineQueueNotice kind="character" count={pendingCharacterCount} />
    </div>
  {/if}

  <div class="character-list-container" data-testid="character-list-container">
    <!-- Character list: M3 Expressive segmented list.
      ［
        （キャラクターアイコン ［
          キャラクター名 説明※120文字以降は……として省略
        ］ アクションボタン ※右寄せ・順序固定）
        ...
      ］
    -->
    <List class="character-list character-segmented-list" data-testid="character-list">
      {#each visibleCharacters as char (char.id)}
        {@const actions = rowActions(char)}
        <ListItem
          type="text"
          onclick={() => openEdit(char)}
          class="character-card-wrapper character-segmented-item"
          data-testid="character-card-{char.id}"
        >
          {#snippet avatar()}
            <button
              type="button"
              class="char-avatar-btn"
              onclick={(e) => {
                e.stopPropagation();
                openEdit(char);
              }}
              disabled={!capabilities.canManageCharacters}
              title="キャラクター編集"
              data-testid="edit-char-btn-{char.id}"
            >
              <img
                class="char-list-avatar"
                src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                alt={char.name}
              />
            </button>
          {/snippet}

          <span class="char-name-label" data-testid="char-name-{char.id}">{char.name}</span>

          {#snippet supportingText()}
            <span class="char-desc-label" data-testid="char-desc-{char.id}">
              {truncateDescription(char.description)}
            </span>
            {#if activeView !== 'scene'}
              <span class="char-scene-label" data-testid="char-scene-{char.id}">
                {characterSceneLabel(char.id)}
              </span>
            {/if}
          {/snippet}

          {#snippet end()}
            <div class="char-actions" onclick={(e) => e.stopPropagation()}>
              {#each actions as action (action)}
                {#if action === 'move'}
                  <IconButton
                    variant="tonal"
                    onclick={() => toggleMenu('move', char.id)}
                    bind:element={menuAnchors[menuKey('move', char.id)]}
                    disabled={isMovePending(char.id)}
                    aria-label="別シーンに配置"
                    title="別シーンに配置"
                    aria-haspopup="menu"
                    aria-expanded={isMenuOpen('move', char.id) ? 'true' : 'false'}
                    data-testid="move-char-btn-{char.id}"
                  >
                    {#if isMovePending(char.id)}
                      <CircularProgress
                        indeterminate
                        wavy
                        track={false}
                        class="character-action-progress"
                        data-testid="move-char-pending-{char.id}"
                        aria-label="送信中"
                      />
                    {:else}
                      <Icon>drive_file_move</Icon>
                    {/if}
                  </IconButton>
                  <Menu
                    bind:element={menuElements[menuKey('move', char.id)]}
                    open={isMenuOpen('move', char.id)}
                    anchor={menuAnchors[menuKey('move', char.id)]}
                    style={getMenuPositionStyle(menuAnchors[menuKey('move', char.id)], {
                      placement: 'bottom-end',
                      anchorName: `--character-menu-anchor-${menuKey('move', char.id)}`,
                    })}
                    data-testid="move-char-menu-{char.id}"
                    class="character-menu"
                    ontoggle={(e) => {
                      if (!e.newState || e.newState === 'closed') {
                        if (isMenuOpen('move', char.id)) closeMenu();
                      }
                    }}
                  >
                    {#each moveTargetScenes(char.id) as targetScene (targetScene.id)}
                      <MenuItem
                        onclick={() => handleMoveCharacter(char, targetScene.id)}
                        data-testid="move-char-scene-{char.id}-{targetScene.id}"
                      >
                        {targetScene.name}
                      </MenuItem>
                    {/each}
                    {#if moveTargetScenes(char.id).length === 0}
                      <div class="character-menu-empty" data-testid="move-char-menu-empty">
                        移動できるシーンがありません
                      </div>
                    {/if}
                  </Menu>
                {:else if action === 'focus'}
                  <IconButton
                    variant="text"
                    onclick={() => handleFocusCharacter(char)}
                    aria-label="視点を合わせる"
                    title="視点を合わせる"
                    data-testid="focus-char-btn-{char.id}"
                  >
                    <Icon>my_location</Icon>
                  </IconButton>
                {:else if action === 'piece'}
                  <CharacterPieceToggle
                    character={char}
                    disabled={!capabilities.canManageCharacters}
                    onToggle={(characterId) => onTogglePiece?.(characterId)}
                  />
                {:else if action === 'more'}
                  <IconButton
                    variant="text"
                    onclick={() => toggleMenu('more', char.id)}
                    bind:element={menuAnchors[menuKey('more', char.id)]}
                    aria-label="その他"
                    title="その他"
                    aria-haspopup="menu"
                    aria-expanded={isMenuOpen('more', char.id) ? 'true' : 'false'}
                    data-testid="more-char-btn-{char.id}"
                  >
                    <Icon>more_vert</Icon>
                  </IconButton>
                  <Menu
                    bind:element={menuElements[menuKey('more', char.id)]}
                    open={isMenuOpen('more', char.id)}
                    anchor={menuAnchors[menuKey('more', char.id)]}
                    style={getMenuPositionStyle(menuAnchors[menuKey('more', char.id)], {
                      placement: 'bottom-end',
                      anchorName: `--character-menu-anchor-${menuKey('more', char.id)}`,
                    })}
                    data-testid="character-menu-{char.id}"
                    class="character-menu"
                    ontoggle={(e) => {
                      if (!e.newState || e.newState === 'closed') {
                        if (isMenuOpen('more', char.id)) closeMenu();
                      }
                    }}
                  >
                    <MenuItem
                      onclick={() => openDeleteConfirm(char)}
                      data-testid="delete-char-btn-{char.id}"
                    >
                      {#snippet start()}<Icon>delete</Icon>{/snippet}
                      削除
                    </MenuItem>
                  </Menu>
                {/if}
              {/each}
            </div>
          {/snippet}
        </ListItem>
      {/each}
    </List>

    {#if isLoading}
      <!-- Contained M3 LoadingIndicator over the cached list while `ListCharacters`
           is in flight (Issue #109 / #144). The list below stays readable. -->
      <div
        class="character-loading-overlay"
        data-testid="character-loading-overlay"
        role="status"
        aria-label="キャラクターを読み込み中"
        aria-live="polite"
      >
        <LoadingIndicator contained data-testid="character-loading-indicator" />
      </div>
    {/if}
  </div>

  <!-- Action failure (move / focus callback): `App`'s scene action error wins. -->
  {#if effectiveActionError}
    <p class="character-action-error" role="status" data-testid="character-action-error">
      {effectiveActionError}
    </p>
  {/if}

  <!-- Character Edit Panel Overlay:
    ［
      （中央寄せ文字列”キャラクター編集”）
      （アイコン※クリックでファイル選択 単一行入力欄”名前” 整数入力欄”イニシアチブ”）
      （複数行入力欄”説明”）
      （単一行入力欄”参照”）
      ［
        ステータス
        （単一行入力欄”ラベル” 整数入力欄”現在値” 整数入力欄”最大値”）
        ...
      ］
      ［
        パラメータ
        （単一行入力欄”ラベル” 整数入力欄”値”）
        ...
      ］
      （複数行入力欄”チャットパレット”）
      （トグル”ステータスを秘匿”）
    ］
  -->
  {#if isEditOpen}
    <CharacterEditModal
      character={editingCharacter}
      isAdmin={capabilities.isAdmin}
      {onRequestAssetPicker}
      onSave={(updated) => {
        isEditOpen = false;
        onUpdateCharacter?.(updated);
      }}
      onClose={() => (isEditOpen = false)}
    />
  {/if}

  <!-- Delete Full-Screen Confirmation Dialog:
    削除ボタン※全画面警告あり
  -->
  {#if isDeleteConfirmOpen && characterToDelete}
    <div
      class="fullscreen-warning-overlay"
      data-testid="char-delete-confirm-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) isDeleteConfirmOpen = false;
      }}
    >
      <div class="delete-warning-box">
        <h2 class="warning-title-centered" data-testid="char-delete-warning-title">
          キャラクター「{characterToDelete.name}」を削除しますか？
        </h2>
        <p class="warning-desc-centered" data-testid="char-delete-warning-desc">
          このキャラクターを削除すると、チャットパレットやステータス情報を含め、すべて消去されます。元に戻すことはできません。
        </p>

        <div class="warning-actions">
          <Button variant="text" onclick={() => (isDeleteConfirmOpen = false)}>キャンセル</Button>
          <button
            type="button"
            class="danger-confirm-btn"
            onclick={confirmDelete}
            data-testid="confirm-char-delete-button"
          >
            削除する
          </button>
        </div>
      </div>
    </div>
  {/if}

</div>

<style>
  .character-tab-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
    box-sizing: border-box;
    overflow-y: auto;
    background: var(--np-color-surface);
    gap: 16px;
  }
  .tab-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .tab-title {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
  }
  .character-view-tabs {
    display: flex;
    width: 100%;
    overflow-x: auto;
  }
  .character-view-tabs :global(.np-segmented-buttons) {
    width: 100%;
    max-width: 100%;
  }
  .character-list-container {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  /*
   * Queue state (timeout / queued save): the wavy notice sits above the list so
   * it stays visible even when cached characters fill the tab.
   */
  .character-queue-notice {
    flex: none;
  }
  /*
   * Contained M3 LoadingIndicator floating over the cached character list while
   * `ListCharacters` is in flight. The surface scrim keeps the cached rows
   * readable without letting the spinner disappear into the background.
   */
  .character-loading-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    background-color: color-mix(in srgb, var(--np-color-surface) 72%, transparent);
  }
  /* M3 Expressive segmented list (Issue #228): 8px gap, surface-container-low,
     outline-variant outline, 12px corner. */
  :global(.character-segmented-list) {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    padding: 0;
    margin: 0;
    list-style: none;
  }
  :global(.character-card-wrapper.character-segmented-item.np-item) {
    display: grid !important;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    gap: 8px 12px;
    align-items: center;
    width: 100%;
    min-height: unset !important;
    padding: 12px !important;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-surface-container-low);
    border: 1px solid var(--np-color-outline-variant);
    box-sizing: border-box;
    cursor: pointer;
    transition: background-color var(--np-motion-expressive-fast-effects, 150ms ease),
                border-color var(--np-motion-expressive-fast-effects, 150ms ease);
  }
  :global(.character-card-wrapper.character-segmented-item.np-item:hover) {
    background: var(--np-color-surface-container);
  }
  :global(.character-segmented-item .np-item-avatar) {
    grid-column: 1;
    grid-row: 1;
    width: 40px;
    height: 40px;
    border-radius: var(--np-shape-corner-full);
    border: 1px solid var(--np-color-outline-variant);
    overflow: hidden;
  }
  :global(.character-segmented-item .np-item-text) {
    grid-column: 2;
    grid-row: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  :global(.character-segmented-item .np-item-end) {
    grid-column: 1 / -1;
    grid-row: 2;
    width: 100%;
    display: flex;
    justify-content: flex-end;
  }
  .char-avatar-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
  }
  .char-avatar-btn:hover .char-list-avatar,
  .char-avatar-btn:focus-visible .char-list-avatar {
    transform: scale(1.1);
    box-shadow: var(--np-elevation-2);
  }
  .char-list-avatar {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    background: var(--np-color-surface-container-highest);
    transition: transform var(--np-motion-expressive-fast-spatial, 200ms ease),
                box-shadow var(--np-motion-expressive-fast-spatial, 200ms ease);
    transform: scale(1);
    will-change: transform;
  }
  .char-name-label {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
  }
  .char-desc-label {
    margin: 0;
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .char-scene-label {
    font-size: 0.72rem;
    color: var(--np-color-on-surface-variant);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .char-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    flex-shrink: 0;
  }
  :global(.character-action-progress) {
    --np-circular-progress-size: 1.25rem;
  }
  .character-menu-empty {
    padding: 8px 16px;
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
  }
  .character-action-error {
    margin: 0;
    padding: 0 8px;
    text-align: center;
    font-size: 0.85rem;
    line-height: 1.3;
    color: var(--np-color-error);
  }

  .fullscreen-warning-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
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
