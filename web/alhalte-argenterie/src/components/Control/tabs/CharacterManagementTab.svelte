<script lang="ts">
  /**
   * Character Management Tab & Character Edit Panel.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
   */
  import { Button, Card, Switch } from 'noph-ui';
  import type { Character } from '../../../types/index.js';
  import { truncateDescription } from '../../../utils/character.js';

  let {
    characters = [],
    onUpdateCharacter,
    onCreateCharacter,
    onDeleteCharacter,
    onTogglePiece,
  }: {
    characters?: Character[];
    onUpdateCharacter?: (character: Character) => void;
    onCreateCharacter?: (character: Character) => void;
    onDeleteCharacter?: (characterId: string) => void;
    onTogglePiece?: (characterId: string) => void;
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
    const url = prompt('アイコン画像URLまたはファイルパスを入力:', editingCharacter?.iconUrl || '');
    if (url !== null && editingCharacter) {
      editingCharacter.iconUrl = url;
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
    <Button variant="filled" onclick={handleCreateNewChar} data-testid="create-character-btn">
      新規作成
    </Button>
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

            <!-- Action buttons: Delete & Place as Piece -->
            <div class="char-actions">
              <Button
                variant={char.placedAsPiece ? 'filled' : 'outlined'}
                onclick={() => onTogglePiece?.(char.id)}
                data-testid="place-piece-btn-{char.id}"
              >
                {char.placedAsPiece ? '駒を戻す' : '駒として配置'}
              </Button>
              <button
                type="button"
                class="delete-icon-btn"
                onclick={() => openDeleteConfirm(char)}
                data-testid="delete-char-btn-{char.id}"
                title="キャラクター削除"
              >
                🗑
              </button>
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

        <!-- Avatar, Name, Initiative row -->
        <div class="edit-top-row">
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
            />
            <span class="avatar-hint">画像選択</span>
          </button>

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
</div>

<style>
  .character-tab-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
    box-sizing: border-box;
    overflow-y: auto;
    background: var(--md-sys-color-surface, #fbf8fd);
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
    background: #e0e0e0;
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
    color: var(--md-sys-color-on-surface, #1b1b1f);
  }
  .char-desc-label {
    margin: 0;
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .char-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .delete-icon-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 1.1rem;
    color: var(--md-sys-color-error, #b3261e);
    padding: 6px;
  }
  .edit-panel-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .edit-dialog-content {
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    border-radius: 28px;
    padding: 24px;
    width: 90%;
    max-width: 580px;
    max-height: 90vh;
    overflow-y: auto;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .edit-header-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
  }
  .edit-top-row {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .avatar-picker-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    cursor: pointer;
  }
  .edit-avatar {
    width: 64px;
    height: 64px;
    border-radius: 12px;
    object-fit: cover;
    background: #ccc;
  }
  .avatar-hint {
    font-size: 0.7rem;
    color: var(--md-sys-color-primary, #6750a4);
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
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 8px;
    font-size: 0.9rem;
    background: #fff;
  }
  .multiline-input {
    padding: 8px 12px;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 8px;
    font-size: 0.9rem;
    font-family: inherit;
    background: #fff;
  }
  .palette-area {
    font-family: monospace;
  }
  .sub-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: rgba(255, 255, 255, 0.5);
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
  }
  .add-mini-btn {
    border: none;
    background: transparent;
    color: var(--md-sys-color-primary, #6750a4);
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
    color: #888;
  }
  .remove-mini-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: #888;
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
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1200;
  }
  .delete-warning-box {
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    border-radius: 28px;
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .warning-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.25rem;
    color: var(--md-sys-color-error, #b3261e);
  }
  .warning-desc-centered {
    margin: 0;
    text-align: center;
    font-size: 0.9rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
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
    background: var(--md-sys-color-error, #b3261e);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
</style>
