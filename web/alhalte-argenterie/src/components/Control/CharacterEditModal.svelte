<script lang="ts">
  /**
   * Character Edit Modal (Full-screen scrim overlay at root viewport level).
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
   * - GitHub Issue #52: アイコン画像選択UIの分離（Carousel不要、ホバーズームとAssetPickerModal直接起動）
   * - GitHub Issue #60: キャラクター編集モーダルの全画面化（ビューポート最前面 scrim 上の中央表示）
   * - GitHub Issue #129: キャラクター編集画面についての提言（M3 Switch, M3 Divider, 地の文トグル, 説明文, スクロール対応）
   */
  import { Button, Divider, Switch, TextField } from 'noph-ui';
  import type { Character } from '../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../utils/assets.js';
  import AssetPickerModal from './AssetPickerModal.svelte';
  import IconImagePicker from './IconImagePicker.svelte';

  let {
    character,
    isAdmin = false,
    onRequestAssetPicker,
    onSave,
    onClose,
  }: {
    character: Character;
    isAdmin?: boolean;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    onSave?: (updated: Character) => void;
    onClose?: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let editingCharacter = $state<Character>({
    ...JSON.parse(JSON.stringify(character)),
    hideStatus: Boolean(character.hideStatus),
    isNarrative: Boolean(character.isNarrative),
  });
  let isAssetPickerOpen = $state(false);

  $effect(() => {
    editingCharacter = {
      ...JSON.parse(JSON.stringify(character)),
      hideStatus: Boolean(character.hideStatus),
      isNarrative: Boolean(character.isNarrative),
    };
  });

  function addStatusRow() {
    editingCharacter.statuses.push({ label: '', current: 0, max: 0 });
  }

  function removeStatusRow(index: number) {
    editingCharacter.statuses.splice(index, 1);
  }

  function addParamRow() {
    editingCharacter.parameters.push({ label: '', value: 0 });
  }

  function removeParamRow(index: number) {
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
    editingCharacter.iconUrl = dataUrl;
  }

  function handleSave() {
    onSave?.(JSON.parse(JSON.stringify(editingCharacter)));
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="character-edit-overlay edit-panel-overlay fullscreen-dialog-overlay"
  data-testid="character-edit-panel"
  style="overflow-y: auto; overscroll-behavior: contain;"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose?.();
  }}
>
  <div
    class="edit-dialog-content"
    style="overflow-y: auto; max-height: 85vh; overscroll-behavior: contain;"
  >
    <h2 class="edit-header-centered" data-testid="edit-dialog-title">キャラクター編集</h2>

    <!-- Avatar, Name, Initiative row -->
    <div class="edit-top-row">
      <div class="avatar-picker-column">
        <IconImagePicker
          src={editingCharacter.iconUrl}
          alt={editingCharacter.name}
          size={64}
          title="クリックでファイル選択"
          dataTestId="edit-avatar-picker"
          imgTestId="edit-avatar-img"
          {isAdmin}
          onRequestAssetPicker={onRequestAssetPicker}
          onSelect={handleIconPicked}
        />
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

    <Divider />

    <!-- Status list -->
    <div class="sub-section">
      <div class="sub-section-header">
        <span class="sub-section-title">ステータス</span>
        <button type="button" class="add-mini-btn" onclick={addStatusRow} data-testid="add-status-btn">＋追加</button>
      </div>
      <p class="sub-section-desc" data-testid="status-section-desc">体力などのゲーム中に頻繁に更新される値です。</p>
      <Divider />
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

    <Divider />

    <!-- Parameter list -->
    <div class="sub-section">
      <div class="sub-section-header">
        <span class="sub-section-title">パラメータ</span>
        <button type="button" class="add-mini-btn" onclick={addParamRow} data-testid="add-param-btn">＋追加</button>
      </div>
      <p class="sub-section-desc" data-testid="param-section-desc">筋力、知力などゲーム中めったに更新されない値です。</p>
      <Divider />
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

    <Divider />

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

    <Divider />

    <!-- Toggle: 地の文 -->
    <div class="toggle-setting-row">
      <span class="label-text">地の文</span>
      <Switch
        bind:selected={editingCharacter.isNarrative}
        data-testid="edit-narrative-toggle"
        aria-label="地の文"
      />
    </div>

    <!-- Toggle: ステータスを秘匿 -->
    <div class="toggle-setting-row">
      <span class="label-text">ステータスを秘匿</span>
      <Switch
        bind:selected={editingCharacter.hideStatus}
        data-testid="edit-hide-status-toggle"
        aria-label="ステータスを秘匿"
      />
    </div>

    <Divider />

    <!-- Dialog buttons -->
    <div class="dialog-actions">
      <Button variant="text" onclick={() => onClose?.()} data-testid="cancel-character-btn">キャンセル</Button>
      <Button variant="filled" onclick={handleSave} data-testid="save-character-btn">
        保存する
      </Button>
    </div>
  </div>
</div>

{#if isAssetPickerOpen}
  <AssetPickerModal
    open={isAssetPickerOpen}
    initialMediaType="image"
    {isAdmin}
    onSelect={handleIconPicked}
    onClose={() => (isAssetPickerOpen = false)}
  />
{/if}

<style>
  .character-edit-overlay {
    position: fixed;
    inset: 0;
    z-index: 9000;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    padding: 16px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .edit-dialog-content {
    background: var(--np-color-surface-container-high);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    padding: 24px;
    width: 90%;
    max-width: 580px;
    max-height: 85vh;
    overflow-y: auto;
    overscroll-behavior: contain;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin: auto;
  }
  .edit-header-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
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
    gap: 8px;
    background: var(--np-color-surface-container);
    padding: 12px;
    border-radius: 12px;
  }
  .sub-section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sub-section-title {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
  }
  .sub-section-desc {
    margin: 0;
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.4;
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
    padding: 4px 0;
  }
  .label-text {
    font-size: 0.95rem;
    font-weight: 500;
    color: var(--np-color-on-surface);
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
</style>
