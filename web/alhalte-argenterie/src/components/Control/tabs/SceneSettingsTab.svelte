<script lang="ts">
  /**
   * Scene Settings Tab & Overlays.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成 / メンバーを移動`
   * - GitHub Issue #32: Material 3 Expressive Switch, Slider, Carousel, 7:5:3 row & Material Color delete button
   */
  import { Button, Carousel, CarouselItem, Slider, Switch, TextField } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import GenericImagePicker from '../GenericImagePicker.svelte';
  import type { Character, Scene } from '../../../types/index.js';
  import {
    loadAssets,
    subscribeAssets,
    type AssetItem,
    type AssetMediaType,
  } from '../../../utils/assets.js';
  import AssetPickerModal from '../AssetPickerModal.svelte';

  let {
    currentScene,
    allScenes = [],
    characters = [],
    isAdmin = false,
    onUpdateScene,
    onSelectScene,
    onCreateScene,
    onDeleteScene,
    onMoveMember,
    onRequestDeleteScene,
    onRequestDeleteSceneLogs,
    onDeleteSceneLogs,
    onRequestAssetPicker,
  }: {
    currentScene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    isAdmin?: boolean;
    onUpdateScene?: (scene: Scene) => void;
    onSelectScene?: (sceneId: string) => void;
    onCreateScene?: (name: string, inheritJuice: boolean) => void;
    onDeleteScene?: (sceneId: string) => void;
    onMoveMember?: (characterId: string, targetSceneName: string) => void;
    onRequestDeleteScene?: (scene: { id: string; name: string }) => void;
    onRequestDeleteSceneLogs?: (scene: { id: string; name: string }) => void;
    onDeleteSceneLogs?: (sceneId: string) => void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
  } = $props();

  let isSceneSplitMenuOpen = $state(false);

  // Asset picker state
  let isAssetPickerOpen = $state(false);
  let assetPickerTarget = $state<'bgImage' | 'bgm1' | 'bgm2' | 'bgm3'>('bgImage');
  let assetPickerMediaType = $state<AssetMediaType>('image');
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

  // Overlays
  let isCreateSceneOpen = $state(false);
  let newSceneName = $state('');
  let inheritJuiceToggle = $state(true);

  let isMoveMemberOpen = $state(false);
  let isDeleteConfirmOpen = $state(false);
  let isDeleteLogsConfirmOpen = $state(false);
  let hoveredAction = $state<'create' | 'delete-logs' | 'delete-scene' | null>(null);

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
    const raw = (e.target as HTMLInputElement).value.trim();
    const num = raw === '' ? NaN : Number(raw);
    const parsed = Number.isNaN(num) || num <= 0 ? undefined : num;
    inputWidth = parsed;
    currentScene.juices.bgWidth = parsed;
    onUpdateScene?.(currentScene);
  }

  function handleHeightChange(e: Event) {
    const raw = (e.target as HTMLInputElement).value.trim();
    const num = raw === '' ? NaN : Number(raw);
    const parsed = Number.isNaN(num) || num <= 0 ? undefined : num;
    inputHeight = parsed;
    currentScene.juices.bgHeight = parsed;
    onUpdateScene?.(currentScene);
  }


  function handleBgImageSelect() {
    assetPickerTarget = 'bgImage';
    assetPickerMediaType = 'image';
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'image',
        initialScope: 'personal',
        onSelect: (dataUrl) => {
          handleAssetPicked(dataUrl);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  function handleBgmSelect(bgmKey: 'bgm1' | 'bgm2' | 'bgm3') {
    assetPickerTarget = bgmKey;
    assetPickerMediaType = 'audio';
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'audio',
        initialScope: 'personal',
        onSelect: (dataUrl) => {
          handleAssetPicked(dataUrl);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  function handleAssetPicked(dataUrl: string | undefined) {
    if (assetPickerTarget === 'bgImage') {
      currentScene.juices.bgImage = dataUrl;
    } else {
      currentScene.juices[assetPickerTarget] = dataUrl;
    }
    onUpdateScene?.(currentScene);
  }

  function handleQuickSelectBgImage(dataUrl: string | undefined) {
    currentScene.juices.bgImage = dataUrl;
    onUpdateScene?.(currentScene);
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

  function handleDeleteButtonClick() {
    if (onRequestDeleteScene) {
      onRequestDeleteScene({
        id: currentScene.id,
        name: currentScene.name,
      });
    } else {
      isDeleteConfirmOpen = true;
    }
  }

  function handleDeleteConfirm() {
    isDeleteConfirmOpen = false;
    onDeleteScene?.(currentScene.id);
  }

  function handleDeleteLogsButtonClick() {
    if (onRequestDeleteSceneLogs) {
      onRequestDeleteSceneLogs({
        id: currentScene.id,
        name: currentScene.name,
      });
    } else {
      isDeleteLogsConfirmOpen = true;
    }
  }

  function handleDeleteLogsConfirm() {
    isDeleteLogsConfirmOpen = false;
    onDeleteSceneLogs?.(currentScene.id);
  }

  let sceneSplitContainer = $state<HTMLElement | null>(null);

  function handleWindowClick(e: MouseEvent) {
    if (isSceneSplitMenuOpen && sceneSplitContainer && !sceneSplitContainer.contains(e.target as Node)) {
      isSceneSplitMenuOpen = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="scene-settings-container" data-testid="scene-settings-tab">
  <!-- Top Split Button / Scene Name Input:
    （中央寄せ単一行入力欄としてのシーン名 兼 スプリットボタン［
      ほかのシーン名 ...※タップで設定中のシーンを切り替え
    ］）
  -->
  <div class="split-button-container" bind:this={sceneSplitContainer} data-testid="scene-split-container">
    <div class="scene-name-split-row">
      <TextField
        variant="outlined"
        label="シーン名"
        value={currentScene.name}
        oninput={handleSceneNameInput}
        data-testid="scene-name-input"
        placeholder="シーン名"
        class="scene-name-field"
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

    <div class="setting-item">
      <GenericImagePicker
        label="背景画像アセット"
        aspectRatio={16 / 9}
        showNone={true}
        {isAdmin}
        imageAssets={imageAssets}
        onSelect={handleQuickSelectBgImage}
        onOpenPicker={handleBgImageSelect}
        onRequestAssetPicker={onRequestAssetPicker}
        dataTestId="bg-generic-image-picker"
        dataCarouselTestId="bg-image-carousel"
        noneTestId="bg-carousel-item-none"
        itemTestIdPrefix="bg-carousel-item"
        moreButtonTestId="more-images-btn"
        cardTestId="bg-image-card"
        legacyOpenBtnTestId="open-bg-picker-btn"
      />
      <p class="description-text" data-testid="bg-description-text">
        画像は一方を指定すればもう一方を自動で計算します。もう一方も入力した場合、端を切り落として揃えます。
      </p>

      <div class="dimensions-row">
        <div class="dim-field">
          <TextField
            id="width-input"
            variant="outlined"
            type="number"
            label="横幅"
            placeholder="自動計算"
            suffixText="px"
            value={inputWidth ?? ''}
            oninput={handleWidthChange}
            data-testid="bg-width-input"
            class="dim-text-field"
          />
        </div>
        <div class="dim-field">
          <TextField
            id="height-input"
            variant="outlined"
            type="number"
            label="縦幅"
            placeholder="自動計算"
            suffixText="px"
            value={inputHeight ?? ''}
            oninput={handleHeightChange}
            data-testid="bg-height-input"
            class="dim-text-field"
          />
        </div>
      </div>
    </div>

    <!-- BGM Pills and Sliders -->
    <div class="bgm-controls">
      <!-- BGM 1 -->
      <div class="bgm-row" data-testid="bgm-row-1">
        <Button
          variant="tonal"
          size="s"
          shape="round"
          onclick={() => handleBgmSelect('bgm1')}
          data-testid="bgm-pill-1"
        >
          BGM1
        </Button>
        <div class="slider-wrapper">
          <Slider
            min={0}
            max={1}
            step={0.05}
            bind:value={currentScene.juices.bgm1Volume}
            oninput={() => onUpdateScene?.(currentScene)}
            onchange={() => onUpdateScene?.(currentScene)}
            aria-label="BGM1音量"
            data-testid="bgm-slider-1"
          />
          <span class="vol-label">{Math.round(currentScene.juices.bgm1Volume * 100)}%</span>
        </div>
      </div>

      <!-- BGM 2 -->
      <div class="bgm-row" data-testid="bgm-row-2">
        <Button
          variant="tonal"
          size="s"
          shape="round"
          onclick={() => handleBgmSelect('bgm2')}
          data-testid="bgm-pill-2"
        >
          BGM2
        </Button>
        <div class="slider-wrapper">
          <Slider
            min={0}
            max={1}
            step={0.05}
            bind:value={currentScene.juices.bgm2Volume}
            oninput={() => onUpdateScene?.(currentScene)}
            onchange={() => onUpdateScene?.(currentScene)}
            aria-label="BGM2音量"
            data-testid="bgm-slider-2"
          />
          <span class="vol-label">{Math.round(currentScene.juices.bgm2Volume * 100)}%</span>
        </div>
      </div>

      <!-- BGM 3 -->
      <div class="bgm-row" data-testid="bgm-row-3">
        <Button
          variant="tonal"
          size="s"
          shape="round"
          onclick={() => handleBgmSelect('bgm3')}
          data-testid="bgm-pill-3"
        >
          BGM3
        </Button>
        <div class="slider-wrapper">
          <Slider
            min={0}
            max={1}
            step={0.05}
            bind:value={currentScene.juices.bgm3Volume}
            oninput={() => onUpdateScene?.(currentScene)}
            onchange={() => onUpdateScene?.(currentScene)}
            aria-label="BGM3音量"
            data-testid="bgm-slider-3"
          />
          <span class="vol-label">{Math.round(currentScene.juices.bgm3Volume * 100)}%</span>
        </div>
      </div>
    </div>

    <!-- Heading 管理 -->
    <h3 class="section-heading" data-testid="heading-management">管理</h3>

    <!-- Management buttons in 1 row with 7 : 5 : 3 ratio and soft Material Color for delete -->
    <div class="management-buttons" data-testid="management-buttons">
      <!-- メンバーを移動: M3 Button size="l" -->
      <div class="move-members-row" data-testid="move-members-row">
        <Button
          variant="filled"
          size="l"
          class="move-members-btn"
          onclick={openMoveMembersDialog}
          data-testid="open-move-members-btn"
        >
          {#snippet start()}
            <Icon>group</Icon>
          {/snippet}
          メンバーを移動
        </Button>
      </div>

      <!-- 3つのアクション「シーンを新規作成」「シーンのログを削除」「シーンを削除」:
           横並びの Square L ボタン群として配置。
           初期状態では左ほど大きく、右ほど小さい square の比率。
           マウスホバー時に行全体における自身の占有率がスムーズに拡大し、アイコンの右横にラベル文字が展開表示される。
      -->
      <div
        class="scene-actions-row"
        class:has-hover={hoveredAction !== null}
        data-testid="scene-actions-row"
      >
        <!-- シーンを新規作成 -->
        <div
          class="action-btn-col action-col-create"
          class:hovered={hoveredAction === 'create'}
          style="flex: {hoveredAction === 'create' ? '10 1 0%' : hoveredAction ? '1.5 1 0%' : '5 1 0%'};"
          onmouseenter={() => (hoveredAction = 'create')}
          onmouseleave={() => (hoveredAction = null)}
          data-testid="action-col-create"
        >
          <Button
            variant="tonal"
            shape="square"
            size="l"
            class="action-square-btn create-scene-btn"
            onclick={() => (isCreateSceneOpen = true)}
            aria-label="シーンを新規作成"
            data-testid="open-create-scene-btn"
          >
            {#snippet start()}
              <Icon>add</Icon>
            {/snippet}
            <span class="action-btn-label" data-testid="create-scene-label">シーンを新規作成</span>
          </Button>
        </div>

        <!-- シーンのログを削除 -->
        <div
          class="action-btn-col action-col-delete-logs"
          class:hovered={hoveredAction === 'delete-logs'}
          style="flex: {hoveredAction === 'delete-logs' ? '10 1 0%' : hoveredAction ? '1.5 1 0%' : '3.2 1 0%'};"
          onmouseenter={() => (hoveredAction = 'delete-logs')}
          onmouseleave={() => (hoveredAction = null)}
          data-testid="action-col-delete-logs"
        >
          <Button
            variant="tonal"
            shape="square"
            size="l"
            class="action-square-btn delete-logs-btn destructive-tonal-btn"
            onclick={handleDeleteLogsButtonClick}
            aria-label="シーンのログを削除"
            data-testid="delete-logs-btn"
          >
            {#snippet start()}
              <Icon>delete_sweep</Icon>
            {/snippet}
            <span class="action-btn-label" data-testid="delete-logs-label">ログを削除</span>
          </Button>
        </div>

        <!-- シーンを削除 -->
        <div
          class="action-btn-col action-col-delete-scene"
          class:hovered={hoveredAction === 'delete-scene'}
          style="flex: {hoveredAction === 'delete-scene' ? '10 1 0%' : hoveredAction ? '1.5 1 0%' : '2.2 1 0%'};"
          onmouseenter={() => (hoveredAction = 'delete-scene')}
          onmouseleave={() => (hoveredAction = null)}
          data-testid="action-col-delete-scene"
        >
          <Button
            variant="tonal"
            shape="square"
            size="l"
            class="action-square-btn delete-scene-btn delete-scene-tonal-btn destructive-tonal-btn"
            onclick={handleDeleteButtonClick}
            aria-label="シーンを削除"
            data-testid="delete-scene-btn"
          >
            {#snippet start()}
              <Icon>delete</Icon>
            {/snippet}
            <span class="action-btn-label" data-testid="delete-scene-label">シーンを削除</span>
          </Button>
        </div>
      </div>
    </div>
  </div>

  <!-- Overlay: シーンを新規作成 -->
  {#if isCreateSceneOpen}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="create-scene-overlay"
      onclick={(e) => {
        if (e.target === e.currentTarget) isCreateSceneOpen = false;
      }}
    >
      <div class="dialog-box">
        <h2 class="dialog-title-centered" data-testid="create-scene-title">シーンを新規作成</h2>

        <div class="dialog-input-row">
          <TextField
            id="new-scene-name"
            variant="outlined"
            label="シーン名"
            placeholder="シーン名"
            bind:value={newSceneName}
            data-testid="new-scene-name-input"
            class="new-scene-name-field"
          />
        </div>

        <div class="toggle-row">
          <span class="label-text">Juicesを継承</span>
          <Switch
            bind:selected={inheritJuiceToggle}
            icons="selected"
            aria-label="Juicesを継承"
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
    <div
      class="fullscreen-dialog-overlay"
      data-testid="move-members-overlay"
      onclick={(e) => {
        if (e.target === e.currentTarget) isMoveMemberOpen = false;
      }}
    >
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
                <TextField
                  variant="outlined"
                  label="シーン名"
                  placeholder="シーン名"
                  bind:value={memberTargetSceneNames[char.id]}
                  data-testid="member-target-input-{char.id}"
                  class="member-target-field"
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
    <div
      class="fullscreen-dialog-overlay"
      data-testid="delete-confirm-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) isDeleteConfirmOpen = false;
      }}
    >
      <div class="dialog-box warning-box">
        <h2 class="dialog-title-centered warning-text" data-testid="delete-dialog-title">
          シーン「{currentScene.name}」を削除しますか？
        </h2>
        <p class="warning-body" data-testid="delete-dialog-warning">
          このシーンを削除すると、配置されたログやJuices設定は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isDeleteConfirmOpen = false)}>キャンセル</Button>
          <Button
            variant="tonal"
            class="delete-scene-tonal-btn"
            onclick={handleDeleteConfirm}
            data-testid="confirm-delete-button"
          >
            削除する
          </Button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Overlay: シーンのログを削除 Full-Screen Confirmation Dialog -->
  {#if isDeleteLogsConfirmOpen}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="delete-logs-confirm-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) isDeleteLogsConfirmOpen = false;
      }}
    >
      <div class="dialog-box warning-box">
        <h2 class="dialog-title-centered warning-text" data-testid="delete-logs-dialog-title">
          シーン「{currentScene.name}」のログを削除しますか？
        </h2>
        <p class="warning-body" data-testid="delete-logs-dialog-warning">
          このシーンのすべての会話ログを削除します。この操作は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button variant="text" onclick={() => (isDeleteLogsConfirmOpen = false)} data-testid="cancel-delete-logs-btn">
            キャンセル
          </Button>
          <Button
            variant="tonal"
            class="delete-scene-tonal-btn destructive-tonal-btn"
            onclick={handleDeleteLogsConfirm}
            data-testid="confirm-delete-logs-button"
          >
            削除する
          </Button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Overlay: アセット選択 (AssetPickerModal) -->
  {#if isAssetPickerOpen}
    <AssetPickerModal
      open={isAssetPickerOpen}
      initialMediaType={assetPickerMediaType}
      {isAdmin}
      onSelect={handleAssetPicked}
      onClose={() => (isAssetPickerOpen = false)}
    />
  {/if}
</div>

<style>
  .scene-settings-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--np-color-surface);
    color: var(--np-color-on-surface);
  }
  .split-button-container {
    position: relative;
    padding: 12px 16px;
    background: var(--np-color-surface-container);
    border-bottom: 1px solid var(--np-color-outline-variant);
  }
  .scene-name-split-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .scene-name-split-row :global(.scene-name-field) {
    flex: 1;
  }
  .split-arrow-btn {
    border: 1px solid var(--np-color-outline);
    border-radius: var(--np-shape-corner-small);
    background: var(--np-color-surface-container);
    padding: 14px 16px;
    cursor: pointer;
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
  }
  .split-dropdown-menu {
    position: absolute;
    top: 100%;
    left: 16px;
    right: 16px;
    margin-top: 4px;
    background: var(--np-color-surface-container-high);
    border: 1px solid var(--np-color-outline-variant);
    border-radius: var(--np-shape-corner-medium);
    box-shadow: var(--np-elevation-2);
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
    color: var(--np-color-on-surface);
  }
  .split-menu-item:hover {
    background: var(--np-color-surface-container-highest);
  }
  .split-menu-item.active-scene {
    font-weight: 700;
    color: var(--np-color-primary);
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
    color: var(--np-color-on-surface);
  }
  .setting-item {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .description-text {
    margin: 0;
    font-size: 0.82rem;
    color: var(--np-color-on-surface-variant);
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
  .dim-field :global(.dim-text-field) {
    width: 100%;
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
  .slider-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .slider-wrapper :global(.np-slider) {
    flex: 1;
  }
  .vol-label {
    font-size: 0.8rem;
    width: 36px;
    text-align: right;
    color: var(--np-color-on-surface-variant);
  }
  .management-buttons {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .move-members-row {
    width: 100%;
  }
  .move-members-row :global(.move-members-btn) {
    width: 100%;
    min-height: 48px;
  }
  .scene-actions-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    width: 100%;
    overflow: hidden;
  }
  .action-btn-col {
    display: flex;
    min-width: 0;
    transition: flex var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
                flex-grow var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }
  .action-col-create {
    flex: 5 1 0%;
  }
  .action-col-delete-logs {
    flex: 3.2 1 0%;
  }
  .action-col-delete-scene {
    flex: 2.2 1 0%;
  }
  .action-btn-col:hover,
  .action-btn-col.hovered,
  .action-btn-col:focus-within {
    flex: 10 1 0% !important;
  }
  .scene-actions-row.has-hover .action-btn-col:not(.hovered):not(:focus-within),
  .scene-actions-row:hover .action-btn-col:not(:hover):not(:focus-within) {
    flex: 1.5 1 0%;
  }
  :global(.action-square-btn) {
    width: 100%;
    min-width: 0 !important;
    --_button-min-width: 0;
    --np-button-padding: 0.5rem;
    padding-inline: 0.5rem !important;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: clip;
    transition: all var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }
  :global(.action-square-btn .children-wrapper) {
    min-width: 0;
    overflow: hidden;
    display: inline-flex;
    align-items: center;
  }
  .action-btn-label {
    display: inline-block;
    max-width: 0;
    opacity: 0;
    overflow: hidden;
    white-space: nowrap;
    margin-left: 0;
    font-size: 0.95rem;
    font-weight: 600;
    vertical-align: middle;
    transition: max-width var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
                opacity var(--np-motion-expressive-fast-effects, 150ms ease),
                margin-left var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }
  .action-btn-col:hover .action-btn-label,
  .action-btn-col.hovered .action-btn-label,
  .action-btn-col:focus-within .action-btn-label {
    max-width: 180px;
    opacity: 1;
    margin-left: 6px;
  }
  :global(.delete-scene-tonal-btn),
  :global(.destructive-tonal-btn) {
    --np-tonal-button-container-color: var(--np-color-error-container, #ffdad6);
    --np-tonal-button-label-text-color: var(--np-color-on-error-container, #410002);
    --np-button-container-color: var(--np-color-error-container, #ffdad6);
    --np-button-label-text-color: var(--np-color-on-error-container, #410002);
  }
  .fullscreen-dialog-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .dialog-box {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    border-radius: var(--np-shape-corner-extra-large);
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3);
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
    color: var(--np-color-on-surface);
  }
  .dialog-input-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dialog-input-row :global(.new-scene-name-field) {
    width: 100%;
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
    background: var(--np-color-surface);
    border-radius: var(--np-shape-corner-medium);
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
    border-radius: var(--np-shape-corner-full);
  }
  .member-name {
    font-size: 0.9rem;
    font-weight: 500;
  }
  .arrow-symbol {
    color: var(--np-color-primary);
    font-weight: 700;
  }
  .split-select-wrapper {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
  }
  .split-select-wrapper :global(.member-target-field) {
    width: 100%;
  }
  .inline-arrow-btn {
    position: absolute;
    right: 4px;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 4px;
    color: var(--np-color-on-surface-variant);
  }
  .inline-scene-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    margin-top: 2px;
    background: var(--np-color-surface-container-high);
    border: 1px solid var(--np-color-outline-variant);
    border-radius: var(--np-shape-corner-small);
    box-shadow: var(--np-elevation-2);
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
    color: var(--np-color-on-surface);
  }
  .dropdown-scene-opt:hover {
    background: var(--np-color-surface-container-highest);
  }
  .warning-text {
    color: var(--np-color-tertiary);
  }
  .warning-body {
    margin: 0;
    font-size: 0.92rem;
    color: var(--np-color-on-surface-variant);
    text-align: center;
  }
</style>
