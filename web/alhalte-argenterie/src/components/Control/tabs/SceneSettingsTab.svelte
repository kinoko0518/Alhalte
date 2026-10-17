<script lang="ts">
  /**
   * Scene Settings Tab & Overlays.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成 / メンバーを移動`
   */
  import { Button, Card, Slider, Switch } from 'noph-ui';
  import type { Character, Scene } from '../../../types/index.js';
  import {
    calculateDimensionsAndCrop,
    inheritJuices,
    moveMemberToScene,
  } from '../../../utils/sceneSettings.js';

  let {
    currentScene,
    allScenes = [],
    characters = [],
    onUpdateScene,
    onSelectScene,
    onCreateScene,
    onDeleteScene,
    onMoveMember,
  }: {
    currentScene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    onUpdateScene?: (scene: Scene) => void;
    onSelectScene?: (sceneId: string) => void;
    onCreateScene?: (name: string, inheritJuice: boolean) => void;
    onDeleteScene?: (sceneId: string) => void;
    onMoveMember?: (characterId: string, targetSceneName: string) => void;
  } = $props();

  let isSceneSplitMenuOpen = $state(false);

  // Overlays
  let isCreateSceneOpen = $state(false);
  let newSceneName = $state('');
  let inheritJuiceToggle = $state(true);

  let isMoveMemberOpen = $state(false);
  let isDeleteConfirmOpen = $state(false);

  // Per-character member move target names: characterId -> sceneName
  let memberTargetSceneNames = $state<Record<string, string>>({});
  let activeMemberSplitId = $state<string | null>(null);

  // Dimension inputs
  let inputWidth = $state<number | undefined>(undefined);
  let inputHeight = $state<number | undefined>(undefined);

  // Keep dimension inputs in sync with currentScene
  $effect(() => {
    inputWidth = currentScene.juices.bgWidth;
    inputHeight = currentScene.juices.bgHeight;
  });

  function handleWidthChange(e: Event) {
    const val = (e.target as HTMLInputElement).valueAsNumber;
    inputWidth = Number.isNaN(val) ? undefined : val;
    applyDimensions();
  }

  function handleHeightChange(e: Event) {
    const val = (e.target as HTMLInputElement).valueAsNumber;
    inputHeight = Number.isNaN(val) ? undefined : val;
    applyDimensions();
  }

  function applyDimensions() {
    const naturalWidth = currentScene.juices.bgWidth || 1920;
    const naturalHeight = currentScene.juices.bgHeight || 1080;
    const res = calculateDimensionsAndCrop({
      naturalWidth,
      naturalHeight,
      width: inputWidth,
      height: inputHeight,
    });
    currentScene.juices.bgWidth = res.width;
    currentScene.juices.bgHeight = res.height;
    onUpdateScene?.(currentScene);
  }

  function handleBgImageSelect() {
    // In real browser this opens file dialog; here we prompt or set placeholder
    const url = prompt('背景画像URLを入力またはファイル指定:', currentScene.juices.bgImage || '');
    if (url !== null) {
      currentScene.juices.bgImage = url;
      onUpdateScene?.(currentScene);
    }
  }

  function handleBgmSelect(bgmKey: 'bgm1' | 'bgm2' | 'bgm3') {
    const url = prompt(`${bgmKey.toUpperCase()} 音声URLまたはファイル指定:`, currentScene.juices[bgmKey] || '');
    if (url !== null) {
      currentScene.juices[bgmKey] = url;
      onUpdateScene?.(currentScene);
    }
  }

  function handleSceneNameInput(e: Event) {
    currentScene.name = (e.target as HTMLInputElement).value;
    onUpdateScene?.(currentScene);
  }

  function handleCreateSceneSubmit() {
    if (!newSceneName.trim()) return;
    onCreateScene?.(newSceneName.trim(), inheritJuiceToggle);
    newSceneName = '';
    isCreateSceneOpen = false;
  }

  function openMoveMembersDialog() {
    // Initialize targets with current scene name for present members
    const targets: Record<string, string> = {};
    for (const char of characters) {
      const found = allScenes.find((s) => s.members.includes(char.id));
      targets[char.id] = found ? found.name : currentScene.name;
    }
    memberTargetSceneNames = targets;
    isMoveMemberOpen = true;
  }

  function handleMoveMemberApply(charId: string) {
    const targetName = memberTargetSceneNames[charId];
    if (!targetName) return;
    onMoveMember?.(charId, targetName);
  }

  function handleDeleteConfirm() {
    isDeleteConfirmOpen = false;
    onDeleteScene?.(currentScene.id);
  }
</script>

<div class="scene-settings-container" data-testid="scene-settings-tab">
  <!-- Top Split Button / Scene Name Input:
    （中央寄せ単一行入力欄としてのシーン名 兼 スプリットボタン［
      ほかのシーン名 ...※タップで設定中のシーンを切り替え
    ］）
  -->
  <div class="split-button-container" data-testid="scene-split-container">
    <div class="scene-name-split-row">
      <input
        type="text"
        class="scene-name-input-centered"
        value={currentScene.name}
        oninput={handleSceneNameInput}
        data-testid="scene-name-input"
        placeholder="シーン名"
      />
      <button
        type="button"
        class="split-arrow-btn"
        onclick={() => (isSceneSplitMenuOpen = !isSceneSplitMenuOpen)}
        data-testid="scene-split-dropdown-toggle"
        title="シーン切り替え"
      >
        ▼
      </button>
    </div>

    {#if isSceneSplitMenuOpen}
      <div class="split-dropdown-menu" data-testid="scene-split-menu">
        {#each allScenes as sc (sc.id)}
          <button
            type="button"
            class="split-menu-item"
            class:active-scene={sc.id === currentScene.id}
            onclick={() => {
              isSceneSplitMenuOpen = false;
              onSelectScene?.(sc.id);
            }}
            data-testid="scene-option-{sc.id}"
          >
            {sc.name}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="settings-scroll-content">
    <!-- Heading Juices -->
    <h3 class="section-heading" data-testid="heading-juices">Juices</h3>

    <div class="setting-item" data-testid="bg-image-card">
      <button type="button" class="card-button" onclick={handleBgImageSelect}>
        <Card variant="outlined">
          <div class="card-inner">
            <span class="card-label">背景画像</span>
            <span class="card-value">{currentScene.juices.bgImage ? '設定済み' : '未設定 (クリックで選択)'}</span>
          </div>
        </Card>
      </button>
      <p class="description-text" data-testid="bg-description-text">
        画像は一方を指定すればもう一方を自動で計算します。もう一方も入力した場合、端を切り落として揃えます。
      </p>

      <div class="dimensions-row">
        <div class="dim-field">
          <label for="width-input">横幅</label>
          <input
            id="width-input"
            type="number"
            class="num-input"
            placeholder="自動計算"
            value={inputWidth ?? ''}
            oninput={handleWidthChange}
            data-testid="bg-width-input"
          />
        </div>
        <div class="dim-field">
          <label for="height-input">縦幅</label>
          <input
            id="height-input"
            type="number"
            class="num-input"
            placeholder="自動計算"
            value={inputHeight ?? ''}
            oninput={handleHeightChange}
            data-testid="bg-height-input"
          />
        </div>
      </div>
    </div>

    <!-- BGM Pills and Sliders -->
    <div class="bgm-controls">
      <!-- BGM 1 -->
      <div class="bgm-row" data-testid="bgm-row-1">
        <button
          type="button"
          class="pill-button"
          onclick={() => handleBgmSelect('bgm1')}
          data-testid="bgm-pill-1"
        >
          BGM1
        </button>
        <div class="slider-wrapper">
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={currentScene.juices.bgm1Volume}
            oninput={(e) => {
              currentScene.juices.bgm1Volume = Number((e.target as HTMLInputElement).value);
              onUpdateScene?.(currentScene);
            }}
            data-testid="bgm-slider-1"
          />
          <span class="vol-label">{Math.round(currentScene.juices.bgm1Volume * 100)}%</span>
        </div>
      </div>

      <!-- BGM 2 -->
      <div class="bgm-row" data-testid="bgm-row-2">
        <button
          type="button"
          class="pill-button"
          onclick={() => handleBgmSelect('bgm2')}
          data-testid="bgm-pill-2"
        >
          BGM2
        </button>
        <div class="slider-wrapper">
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={currentScene.juices.bgm2Volume}
            oninput={(e) => {
              currentScene.juices.bgm2Volume = Number((e.target as HTMLInputElement).value);
              onUpdateScene?.(currentScene);
            }}
            data-testid="bgm-slider-2"
          />
          <span class="vol-label">{Math.round(currentScene.juices.bgm2Volume * 100)}%</span>
        </div>
      </div>

      <!-- BGM 3 -->
      <div class="bgm-row" data-testid="bgm-row-3">
        <button
          type="button"
          class="pill-button"
          onclick={() => handleBgmSelect('bgm3')}
          data-testid="bgm-pill-3"
        >
          BGM3
        </button>
        <div class="slider-wrapper">
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={currentScene.juices.bgm3Volume}
            oninput={(e) => {
              currentScene.juices.bgm3Volume = Number((e.target as HTMLInputElement).value);
              onUpdateScene?.(currentScene);
            }}
            data-testid="bgm-slider-3"
          />
          <span class="vol-label">{Math.round(currentScene.juices.bgm3Volume * 100)}%</span>
        </div>
      </div>
    </div>

    <!-- Heading 管理 -->
    <h3 class="section-heading" data-testid="heading-management">管理</h3>

    <div class="management-buttons" data-testid="management-buttons">
      <Button variant="outlined" onclick={openMoveMembersDialog} data-testid="open-move-members-btn">
        メンバーを移動
      </Button>
      <Button
        variant="outlined"
        onclick={() => (isCreateSceneOpen = true)}
        data-testid="open-create-scene-btn"
      >
        シーンを新規作成
      </Button>
      <button
        type="button"
        class="delete-scene-red-btn"
        onclick={() => (isDeleteConfirmOpen = true)}
        data-testid="delete-scene-btn"
      >
        シーンを削除
      </button>
    </div>
  </div>

  <!-- Overlay: シーンを新規作成 -->
  {#if isCreateSceneOpen}
    <div class="fullscreen-dialog-overlay" data-testid="create-scene-overlay">
      <div class="dialog-box">
        <h2 class="dialog-title-centered" data-testid="create-scene-title">シーンを新規作成</h2>

        <div class="dialog-input-row">
          <label for="new-scene-name" class="label-text">シーン名</label>
          <input
            id="new-scene-name"
            type="text"
            class="text-input"
            placeholder="シーン名"
            bind:value={newSceneName}
            data-testid="new-scene-name-input"
          />
        </div>

        <div class="toggle-row">
          <span class="label-text">Juicesを継承</span>
          <input
            type="checkbox"
            bind:checked={inheritJuiceToggle}
            data-testid="inherit-juices-toggle"
          />
        </div>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isCreateSceneOpen = false)}>キャンセル</Button>
          <Button variant="filled" onclick={handleCreateSceneSubmit} data-testid="submit-create-scene-btn">
            新規作成
          </Button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Overlay: メンバーを移動 -->
  {#if isMoveMemberOpen}
    <div class="fullscreen-dialog-overlay" data-testid="move-members-overlay">
      <div class="dialog-box large-dialog">
        <h2 class="dialog-title-centered" data-testid="move-members-title">メンバーを移動</h2>

        <div class="members-move-list" data-testid="members-move-list">
          {#each characters as char (char.id)}
            <div class="member-move-row" data-testid="move-row-{char.id}">
              <div class="member-info">
                <img
                  class="member-avatar"
                  src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                  alt={char.name}
                />
                <span class="member-name">{char.name}</span>
              </div>

              <span class="arrow-symbol">➔</span>

              <div class="split-select-wrapper">
                <input
                  type="text"
                  class="scene-target-input"
                  bind:value={memberTargetSceneNames[char.id]}
                  placeholder="シーン名"
                  data-testid="member-target-input-{char.id}"
                />
                <button
                  type="button"
                  class="inline-arrow-btn"
                  onclick={() => {
                    activeMemberSplitId =
                      activeMemberSplitId === char.id ? null : char.id;
                  }}
                  data-testid="member-split-toggle-{char.id}"
                >
                  ▼
                </button>

                {#if activeMemberSplitId === char.id}
                  <div class="inline-scene-dropdown" data-testid="member-scene-dropdown-{char.id}">
                    {#each allScenes as sc (sc.id)}
                      <button
                        type="button"
                        class="dropdown-scene-opt"
                        onclick={() => {
                          memberTargetSceneNames[char.id] = sc.name;
                          activeMemberSplitId = null;
                        }}
                      >
                        {sc.name}
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>

              <Button
                variant="tonal"
                onclick={() => handleMoveMemberApply(char.id)}
                data-testid="apply-move-btn-{char.id}"
              >
                移動
              </Button>
            </div>
          {/each}
        </div>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isMoveMemberOpen = false)}>閉じる</Button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Overlay: シーンを削除 Full-Screen Confirmation Dialog -->
  {#if isDeleteConfirmOpen}
    <div class="fullscreen-dialog-overlay" data-testid="delete-confirm-dialog">
      <div class="dialog-box warning-box">
        <h2 class="dialog-title-centered warning-text" data-testid="delete-dialog-title">
          シーン「{currentScene.name}」を削除しますか？
        </h2>
        <p class="warning-body" data-testid="delete-dialog-warning">
          このシーンを削除すると、配置されたログやJuices設定は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isDeleteConfirmOpen = false)}>キャンセル</Button>
          <button
            type="button"
            class="delete-confirm-btn"
            onclick={handleDeleteConfirm}
            data-testid="confirm-delete-button"
          >
            削除する
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .scene-settings-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--md-sys-color-surface, #fbf8fd);
  }
  .split-button-container {
    position: relative;
    padding: 12px 16px;
    background: var(--md-sys-color-surface-container, #f3edf7);
    border-bottom: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
  }
  .scene-name-split-row {
    display: flex;
    align-items: center;
    background: #fff;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 12px;
    overflow: hidden;
  }
  .scene-name-input-centered {
    flex: 1;
    border: none;
    text-align: center;
    font-size: 1.05rem;
    font-weight: 600;
    padding: 10px 14px;
    outline: none;
  }
  .split-arrow-btn {
    border: none;
    background: transparent;
    padding: 10px 14px;
    cursor: pointer;
    border-left: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
  }
  .split-dropdown-menu {
    position: absolute;
    top: 100%;
    left: 16px;
    right: 16px;
    margin-top: 4px;
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    border: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
    border-radius: 12px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    z-index: 250;
    max-height: 200px;
    overflow-y: auto;
  }
  .split-menu-item {
    display: block;
    width: 100%;
    padding: 10px 16px;
    border: none;
    background: none;
    text-align: left;
    font-size: 0.95rem;
    cursor: pointer;
  }
  .split-menu-item:hover {
    background: rgba(103, 80, 164, 0.08);
  }
  .split-menu-item.active-scene {
    font-weight: 700;
    color: var(--md-sys-color-primary, #6750a4);
  }
  .settings-scroll-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .section-heading {
    margin: 8px 0 0;
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--md-sys-color-on-surface, #1b1b1f);
  }
  .setting-item {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card-button {
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
    width: 100%;
  }
  .card-inner {
    padding: 16px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .card-label {
    font-weight: 600;
    font-size: 1rem;
  }
  .card-value {
    color: var(--md-sys-color-primary, #6750a4);
    font-size: 0.88rem;
  }
  .description-text {
    margin: 0;
    font-size: 0.82rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
    line-height: 1.4;
  }
  .dimensions-row {
    display: flex;
    gap: 12px;
  }
  .dim-field {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .dim-field label {
    font-size: 0.85rem;
    font-weight: 500;
  }
  .num-input {
    padding: 8px 12px;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 8px;
    font-size: 0.95rem;
  }
  .bgm-controls {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .bgm-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .pill-button {
    padding: 6px 16px;
    border-radius: 20px;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 600;
  }
  .slider-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .slider-wrapper input {
    flex: 1;
  }
  .vol-label {
    font-size: 0.8rem;
    width: 36px;
    text-align: right;
  }
  .management-buttons {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .delete-scene-red-btn {
    padding: 10px 16px;
    border-radius: 20px;
    border: none;
    background: var(--md-sys-color-error, #b3261e);
    color: #fff;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.2s;
  }
  .delete-scene-red-btn:hover {
    opacity: 0.9;
  }
  .fullscreen-dialog-overlay {
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
  .dialog-box {
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    border-radius: 28px;
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .large-dialog {
    max-width: 640px;
  }
  .dialog-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
  }
  .dialog-input-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .label-text {
    font-size: 0.9rem;
    font-weight: 500;
  }
  .text-input {
    padding: 10px 14px;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 10px;
    font-size: 0.95rem;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .dialog-footer-right {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .members-move-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: 300px;
    overflow-y: auto;
  }
  .member-move-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--md-sys-color-surface, #fff);
    border-radius: 12px;
  }
  .member-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 120px;
  }
  .member-avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
  }
  .member-name {
    font-size: 0.9rem;
    font-weight: 500;
  }
  .arrow-symbol {
    color: var(--md-sys-color-primary, #6750a4);
    font-weight: 700;
  }
  .split-select-wrapper {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
  }
  .scene-target-input {
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 8px;
    font-size: 0.88rem;
  }
  .inline-arrow-btn {
    position: absolute;
    right: 4px;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 4px;
  }
  .inline-scene-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    margin-top: 2px;
    background: #fff;
    border: 1px solid #ccc;
    border-radius: 8px;
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1);
    z-index: 300;
    max-height: 150px;
    overflow-y: auto;
  }
  .dropdown-scene-opt {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    background: none;
    text-align: left;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .dropdown-scene-opt:hover {
    background: #eee;
  }
  .warning-text {
    color: var(--md-sys-color-error, #b3261e);
  }
  .warning-body {
    margin: 0;
    font-size: 0.92rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
    text-align: center;
  }
  .delete-confirm-btn {
    padding: 8px 18px;
    border-radius: 20px;
    border: none;
    background: var(--md-sys-color-error, #b3261e);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
</style>
