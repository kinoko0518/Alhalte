<script lang="ts">
  /**
   * Character Management Tab & Character Edit Panel.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
   */
  import { Button, Carousel, CarouselItem, IconButton, List, ListItem, Switch, TextField } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character } from '../../../types/index.js';
  import {
    loadAssets,
    subscribeAssets,
    type AssetItem,
  } from '../../../utils/assets.js';
  import { truncateDescription } from '../../../utils/character.js';
  import AssetPickerModal from '../AssetPickerModal.svelte';

  let {
    characters = [],
    isAdmin = false,
    onUpdateCharacter,
    onCreateCharacter,
    onDeleteCharacter,
    onTogglePiece,
    onRequestDeleteCharacter,
    onRequestAssetPicker,
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
  } = $props();

  let isEditOpen = $state(false);
  let isAssetPickerOpen = $state(false);
  let allAssets = $state<AssetItem[]>(loadAssets());

  $effect(() => {
    allAssets = loadAssets();
    const unsubscribe = subscribeAssets((updated) => {
      allAssets = updated;
    });
    return unsubscribe;
  });

  let imageAssets = $derived(
    allAssets.filter((a) => a.mediaType === 'image' && (isAdmin || a.scope === 'personal')),
  );

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
  });
  let isDeleteConfirmOpen = $state(false);
  let characterToDelete = $state<Character | null>(null);

  function openEdit(char: Character) {
    // Clone character for editing
    editingCharacter = JSON.parse(JSON.stringify(char));
    isEditOpen = true;
  }

  function closeEdit() {
    isEditOpen = false;
  }

  function handleSaveEdit() {
    isEditOpen = false;
    onUpdateCharacter?.(JSON.parse(JSON.stringify(editingCharacter)));
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

  function addStatusRow() {
    if (!editingCharacter) return;
    editingCharacter.statuses.push({ label: '', current: 0, max: 0 });
  }

  function removeStatusRow(index: number) {
    if (!editingCharacter) return;
    editingCharacter.statuses.splice(index, 1);
  }

  function addParamRow() {
    if (!editingCharacter) return;
    editingCharacter.parameters.push({ label: '', value: 0 });
  }

  function removeParamRow(index: number) {
    if (!editingCharacter) return;
    editingCharacter.parameters.splice(index, 1);
  }

  function handleIconFileSelect() {
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'image',
        initialScope: 'personal',
        onSelect: (dataUrl) => {
          handleIconPicked(dataUrl);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  function handleIconPicked(dataUrl: string | undefined) {
    if (editingCharacter) {
      editingCharacter.iconUrl = dataUrl;
    }
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
              aria-label="駒を削除"
              title="駒を削除"
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
    <div
      class="edit-panel-overlay"
      data-testid="character-edit-panel"
      onclick={(e) => {
        if (e.target === e.currentTarget) closeEdit();
      }}
    >
      <div class="edit-dialog-content">
        <h2 class="edit-header-centered" data-testid="edit-dialog-title">キャラクター編集</h2>

        <!-- Quick-select Image Asset Carousel -->
        <div class="char-carousel-section">
          <div class="char-carousel-wrapper" data-testid="char-image-carousel">
            <Carousel variant="uncontained" label="キャラクター画像アセット">
              <CarouselItem
                type="button"
                label="NONE"
                aspectRatio={1}
                onclick={() => handleIconPicked(undefined)}
                data-testid="char-carousel-item-none"
              />
              {#each imageAssets as asset (asset.id)}
                <CarouselItem
                  type="button"
                  image={asset.dataUrl}
                  label={asset.name}
                  aspectRatio={1}
                  onclick={() => handleIconPicked(asset.dataUrl)}
                  data-testid="char-carousel-item-{asset.id}"
                />
              {/each}
            </Carousel>
          </div>
        </div>

        <!-- Avatar, Name, Initiative row -->
        <div class="edit-top-row">
          <div class="avatar-picker-column">
            <button
              type="button"
              class="avatar-picker-btn"
              onclick={handleIconFileSelect}
              title="クリックでファイル選択"
              data-testid="edit-avatar-picker"
            >
              <img
                class="edit-avatar"
                src={editingCharacter.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                alt={editingCharacter.name}
                data-testid="edit-avatar-img"
              />
            </button>
            <Button
              variant="tonal"
              size="xs"
              onclick={handleIconFileSelect}
              data-testid="open-char-image-picker-btn"
            >
              画像選択
            </Button>
          </div>

          <div class="name-init-fields">
            <TextField
              id="char-edit-name"
              variant="outlined"
              label="名前"
              placeholder="名前"
              bind:value={editingCharacter.name}
              data-testid="edit-name-input"
              class="char-text-field"
            />

            <TextField
              id="char-edit-init"
              variant="outlined"
              type="number"
              label="イニシアチブ"
              placeholder="0"
              bind:value={editingCharacter.initiative}
              data-testid="edit-init-input"
              class="char-text-field init-field"
            />
          </div>
        </div>

        <!-- Description multiline -->
        <div class="field-item">
          <TextField
            id="char-edit-desc"
            variant="outlined"
            type="textarea"
            minLines={3}
            maxLines={6}
            label="説明"
            placeholder="説明"
            bind:value={editingCharacter.description}
            data-testid="edit-description-input"
            class="char-text-field"
          />
        </div>

        <!-- Reference single-line -->
        <div class="field-item">
          <TextField
            id="char-edit-ref"
            variant="outlined"
            label="参照"
            placeholder="参照URL等"
            bind:value={editingCharacter.reference}
            data-testid="edit-reference-input"
            class="char-text-field"
          />
        </div>

        <!-- Status list -->
        <div class="sub-section">
          <div class="sub-section-header">
            <span class="sub-section-title">ステータス</span>
            <button type="button" class="add-mini-btn" onclick={addStatusRow} data-testid="add-status-btn">＋追加</button>
          </div>
          <div class="rows-list" data-testid="edit-status-list">
            {#each editingCharacter.statuses as st, sIdx (sIdx)}
              <div class="status-edit-row">
                <TextField
                  variant="outlined"
                  label="ラベル"
                  placeholder="ラベル"
                  bind:value={st.label}
                  data-testid="status-label-{sIdx}"
                  class="status-label-field"
                />
                <TextField
                  variant="outlined"
                  type="number"
                  label="現在値"
                  placeholder="0"
                  bind:value={st.current}
                  data-testid="status-current-{sIdx}"
                  class="status-num-field"
                />
                <span class="slash-div">/</span>
                <TextField
                  variant="outlined"
                  type="number"
                  label="最大値"
                  placeholder="0"
                  bind:value={st.max}
                  data-testid="status-max-{sIdx}"
                  class="status-num-field"
                />
                <button
                  type="button"
                  class="remove-mini-btn"
                  onclick={() => removeStatusRow(sIdx)}
                >
                  ✕
                </button>
              </div>
            {/each}
          </div>
        </div>

        <!-- Parameter list -->
        <div class="sub-section">
          <div class="sub-section-header">
            <span class="sub-section-title">パラメータ</span>
            <button type="button" class="add-mini-btn" onclick={addParamRow} data-testid="add-param-btn">＋追加</button>
          </div>
          <div class="rows-list" data-testid="edit-param-list">
            {#each editingCharacter.parameters as param, pIdx (pIdx)}
              <div class="param-edit-row">
                <TextField
                  variant="outlined"
                  label="ラベル"
                  placeholder="ラベル"
                  bind:value={param.label}
                  data-testid="param-label-{pIdx}"
                  class="param-label-field"
                />
                <TextField
                  variant="outlined"
                  type="number"
                  label="値"
                  placeholder="0"
                  bind:value={param.value}
                  data-testid="param-value-{pIdx}"
                  class="param-num-field"
                />
                <button
                  type="button"
                  class="remove-mini-btn"
                  onclick={() => removeParamRow(pIdx)}
                >
                  ✕
                </button>
              </div>
            {/each}
          </div>
        </div>

        <!-- Chat Palette multiline -->
        <div class="field-item">
          <TextField
            id="char-edit-palette"
            variant="outlined"
            type="textarea"
            minLines={5}
            maxLines={10}
            label="チャットパレット"
            placeholder="チャットパレット（1行に1コマンド）"
            bind:value={editingCharacter.chatPalette}
            data-testid="edit-chat-palette-input"
            class="char-text-field palette-field"
          />
        </div>

        <!-- Toggle: ステータスを秘匿 -->
        <div class="toggle-setting-row">
          <span class="label-text">ステータスを秘匿</span>
          <input
            type="checkbox"
            bind:checked={editingCharacter.hideStatus}
            data-testid="edit-hide-status-toggle"
          />
        </div>

        <!-- Dialog buttons -->
        <div class="dialog-actions">
          <Button variant="text" onclick={closeEdit}>キャンセル</Button>
          <Button variant="filled" onclick={handleSaveEdit} data-testid="save-character-btn">
            保存する
          </Button>
        </div>
      </div>
    </div>
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

  <!-- Overlay: アセット選択 (AssetPickerModal) -->
  {#if isAssetPickerOpen}
    <AssetPickerModal
      open={isAssetPickerOpen}
      initialMediaType="image"
      {isAdmin}
      onSelect={handleIconPicked}
      onClose={() => (isAssetPickerOpen = false)}
    />
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
  }
  .char-list-avatar {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    background: var(--np-color-surface-container-highest);
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
  .edit-panel-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .edit-dialog-content {
    background: var(--np-color-surface-container-high);
    border-radius: 28px;
    padding: 24px;
    width: 90%;
    max-width: 580px;
    max-height: 90vh;
    overflow-y: auto;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .edit-header-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
  }
  .char-carousel-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .char-carousel-wrapper {
    --np-carousel-item-height: 76px;
    --np-carousel-item-width: 76px;
    --np-carousel-item-container-color: var(--np-color-surface-container-highest);
    --np-carousel-item-outline-width: 1px;
    --np-carousel-item-outline-color: var(--np-color-outline-variant);
  }
  .edit-top-row {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .avatar-picker-column {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .avatar-picker-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
  }
  .edit-avatar {
    width: 64px;
    height: 64px;
    border-radius: 12px;
    object-fit: cover;
    background: var(--np-color-surface-container-highest);
  }
  .name-init-fields {
    flex: 1;
    display: flex;
    gap: 12px;
  }
  .field-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .name-init-fields :global(.char-text-field) {
    flex: 1;
  }
  .name-init-fields :global(.init-field) {
    max-width: 140px;
  }
  .field-item :global(.char-text-field) {
    width: 100%;
  }
  :global(.palette-field textarea) {
    font-family: monospace;
  }
  .sub-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--np-color-surface-container);
    padding: 10px;
    border-radius: 10px;
  }
  .sub-section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sub-section-title {
    font-weight: 600;
    font-size: 0.9rem;
    color: var(--np-color-on-surface);
  }
  .add-mini-btn {
    border: none;
    background: transparent;
    color: var(--np-color-primary);
    font-size: 0.8rem;
    cursor: pointer;
    font-weight: 600;
  }
  .rows-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .status-edit-row,
  .param-edit-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .status-edit-row :global(.status-label-field),
  .param-edit-row :global(.param-label-field) {
    flex: 2;
  }
  .status-edit-row :global(.status-num-field),
  .param-edit-row :global(.param-num-field) {
    flex: 1;
    min-width: 80px;
  }
  .slash-div {
    font-weight: 700;
    color: var(--np-color-on-surface-variant);
  }
  .remove-mini-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: var(--np-color-on-surface-variant);
    padding: 4px;
  }
  .toggle-setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
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
