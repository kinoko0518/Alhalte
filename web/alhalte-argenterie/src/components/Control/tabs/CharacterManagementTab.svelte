<script lang="ts">
  /**
   * Character Management Tab & Character Edit Panel.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
   */
  import { Button, IconButton, List, ListItem } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character } from '../../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../../utils/assets.js';
  import { truncateDescription } from '../../../utils/character.js';
  import CharacterEditModal from '../CharacterEditModal.svelte';

  let {
    characters = [],
    isAdmin = false,
    onUpdateCharacter,
    onCreateCharacter,
    onDeleteCharacter,
    onTogglePiece,
    onRequestDeleteCharacter,
    onRequestAssetPicker,
    onRequestEditCharacter,
  }: {
    characters?: Character[];
    isAdmin?: boolean;
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
  } = $props();

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

  function openEdit(char: Character) {
    if (onRequestEditCharacter) {
      onRequestEditCharacter(char);
    } else {
      editingCharacter = JSON.parse(JSON.stringify(char));
      isEditOpen = true;
    }
  }

  function openDeleteConfirm(char: Character) {
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
    onCreateCharacter?.(newChar);
    openEdit(newChar);
  }
</script>

<div class="character-tab-container" data-testid="character-management-tab">
  <div class="tab-header">
    <h3 class="tab-title">キャラクター一覧</h3>
    <IconButton
      variant="filled"
      onclick={handleCreateNewChar}
      aria-label="新規作成"
      title="新規作成"
      data-testid="create-character-btn"
    >
      <Icon>person_add</Icon>
    </IconButton>
  </div>

  <!-- Character list:
    ［
      （キャラクターアイコン ［
        キャラクター名 説明※120文字以降は……として省略
      ］ 削除ボタン※全画面警告あり 駒として配置ボタン）
      ...
    ］
  -->
  <List class="character-list" data-testid="character-list">
    {#each characters as char (char.id)}
      <ListItem
        type="text"
        onclick={() => openEdit(char)}
        class="character-card-wrapper"
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
        {/snippet}

        {#snippet end()}
          <div class="char-actions" onclick={(e) => e.stopPropagation()}>
            <IconButton
              variant="tonal"
              onclick={() => onTogglePiece?.(char.id)}
              aria-label="駒として配置"
              title="駒として配置"
              data-testid="piece-char-btn-{char.id}"
            >
              <Icon>person_pin_circle</Icon>
            </IconButton>
            <IconButton
              variant="text"
              onclick={() => openDeleteConfirm(char)}
              aria-label="キャラクターを削除"
              title="キャラクターを削除"
              data-testid="delete-char-btn-{char.id}"
            >
              <Icon>delete</Icon>
            </IconButton>
          </div>
        {/snippet}
      </ListItem>
    {/each}
  </List>

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
      {isAdmin}
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
  .character-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    padding: 0;
    margin: 0;
    list-style: none;
  }
  :global(.character-card-wrapper.np-item) {
    width: 100%;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-surface-container-low);
    box-sizing: border-box;
    cursor: pointer;
    transition: background-color var(--np-motion-expressive-fast-effects, 150ms ease);
  }
  :global(.character-card-wrapper.np-item:hover) {
    background: var(--np-color-surface-container);
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
  .char-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
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
