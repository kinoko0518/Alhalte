<script lang="ts">
  /**
   * Character Management Tab & Character Edit Panel.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
   */
  import { Button, Card, Carousel, CarouselItem, IconButton, Switch } from 'noph-ui';
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
  }: {
    characters?: Character[];
    isAdmin?: boolean;
    onUpdateCharacter?: (character: Character) => void;
    onCreateCharacter?: (character: Character) => void;
    onDeleteCharacter?: (characterId: string) => void;
    onTogglePiece?: (characterId: string) => void;
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
    characterToDelete = char;
    isDeleteConfirmOpen = true;
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
    isAssetPickerOpen = true;
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
  <div class="character-list" data-testid="character-list">
    {#each characters as char (char.id)}
      <div class="character-card-wrapper" data-testid="character-card-{char.id}">
        <Card variant="outlined">
          <div class="char-card-body">
            <!-- Icon -->
            <button
              type="button"
              class="char-avatar-btn"
              onclick={() => openEdit(char)}
              title="キャラクター編集"
              data-testid="edit-char-btn-{char.id}"
            >
              <img
                class="char-list-avatar"
                src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                alt={char.name}
              />
            </button>

            <!-- Name and 120-char truncated description -->
            <div
              class="char-text-block"
              onclick={() => openEdit(char)}
              role="button"
              tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && openEdit(char)}
            >
              <span class="char-name-label" data-testid="char-name-{char.id}">{char.name}</span>
              <p class="char-desc-label" data-testid="char-desc-{char.id}">
                {truncateDescription(char.description)}
              </p>
            </div>

            <!-- Action buttons: Place as Piece & Delete -->
            <div class="char-actions">
              <IconButton
                variant={char.placedAsPiece ? 'filled' : 'tonal'}
                toggle
                selected={!!char.placedAsPiece}
                onclick={() => onTogglePiece?.(char.id)}
                aria-label={char.placedAsPiece ? '駒を解除' : '駒として配置'}
                title={char.placedAsPiece ? '駒を解除' : '駒として配置'}
                data-testid="piece-char-btn-{char.id}"
              >
                <Icon>{char.placedAsPiece ? 'person_pin_circle' : 'place_item'}</Icon>
              </IconButton>
              <IconButton
                variant="text"
                onclick={() => openDeleteConfirm(char)}
                aria-label="削除"
                title="削除"
                data-testid="delete-char-btn-{char.id}"
              >
                <Icon>delete</Icon>
              </IconButton>
            </div>
          </div>
        </Card>
      </div>
    {/each}
  </div>

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
    <div class="edit-panel-overlay" data-testid="character-edit-panel">
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
            <div class="field-item">
              <label for="char-edit-name">名前</label>
              <input
                id="char-edit-name"
                type="text"
                class="single-line-input"
                placeholder="名前"
                bind:value={editingCharacter.name}
                data-testid="edit-name-input"
              />
            </div>

            <div class="field-item init-field">
              <label for="char-edit-init">イニシアチブ</label>
              <input
                id="char-edit-init"
                type="number"
                class="single-line-input"
                placeholder="0"
                bind:value={editingCharacter.initiative}
                data-testid="edit-init-input"
              />
            </div>
          </div>
        </div>

        <!-- Description multiline -->
        <div class="field-item">
          <label for="char-edit-desc">説明</label>
          <textarea
            id="char-edit-desc"
            class="multiline-input"
            rows="3"
            placeholder="説明"
            bind:value={editingCharacter.description}
            data-testid="edit-description-input"
          ></textarea>
        </div>

        <!-- Reference single-line -->
        <div class="field-item">
          <label for="char-edit-ref">参照</label>
          <input
            id="char-edit-ref"
            type="text"
            class="single-line-input"
            placeholder="参照URL等"
            bind:value={editingCharacter.reference}
            data-testid="edit-reference-input"
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
                <input
                  type="text"
                  class="single-line-input mini-input"
                  placeholder="ラベル"
                  bind:value={st.label}
                  data-testid="status-label-{sIdx}"
                />
                <input
                  type="number"
                  class="single-line-input num-mini-input"
                  placeholder="現在値"
                  bind:value={st.current}
                  data-testid="status-current-{sIdx}"
                />
                <span class="slash-div">/</span>
                <input
                  type="number"
                  class="single-line-input num-mini-input"
                  placeholder="最大値"
                  bind:value={st.max}
                  data-testid="status-max-{sIdx}"
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
                <input
                  type="text"
                  class="single-line-input mini-input"
                  placeholder="ラベル"
                  bind:value={param.label}
                  data-testid="param-label-{pIdx}"
                />
                <input
                  type="number"
                  class="single-line-input num-mini-input"
                  placeholder="値"
                  bind:value={param.value}
                  data-testid="param-value-{pIdx}"
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
          <label for="char-edit-palette">チャットパレット</label>
          <textarea
            id="char-edit-palette"
            class="multiline-input palette-area"
            rows="5"
            placeholder="チャットパレット（1行に1コマンド）"
            bind:value={editingCharacter.chatPalette}
            data-testid="edit-chat-palette-input"
          ></textarea>
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
    <div class="fullscreen-warning-overlay" data-testid="char-delete-confirm-dialog">
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
    gap: 12px;
  }
  .char-card-body {
    padding: 12px;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .char-avatar-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    flex-shrink: 0;
  }
  .char-list-avatar {
    width: 48px;
    height: 48px;
    border-radius: 10px;
    object-fit: cover;
    background: var(--np-color-surface-container-highest);
  }
  .char-text-block {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    cursor: pointer;
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
    gap: 8px;
    flex-shrink: 0;
  }
  .edit-panel-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--np-color-scrim, var(--np-color-on-surface)) 50%, transparent);
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
  .init-field {
    max-width: 120px;
  }
  .single-line-input {
    padding: 8px 12px;
    border: 1px solid var(--np-color-outline);
    border-radius: 8px;
    font-size: 0.9rem;
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-on-surface);
  }
  .multiline-input {
    padding: 8px 12px;
    border: 1px solid var(--np-color-outline);
    border-radius: 8px;
    font-size: 0.9rem;
    font-family: inherit;
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-on-surface);
  }
  .palette-area {
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
  .mini-input {
    flex: 1;
  }
  .num-mini-input {
    width: 70px;
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
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--np-color-scrim, var(--np-color-on-surface)) 60%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1200;
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
