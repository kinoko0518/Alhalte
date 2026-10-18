<script lang="ts">
  /**
   * Scene Settings Tab & Overlays.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成 / メンバーを移動`
   * - GitHub Issue #32: Material 3 Expressive Switch, Slider, Carousel, 7:5:3 row & Material Color delete button
   */
  import {
    Button,
    Carousel,
    CarouselItem,
    IconButton,
    List,
    ListItem,
    Menu,
    MenuItem,
    Slider,
    Switch,
    TextField,
  } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import GenericImagePicker from '../GenericImagePicker.svelte';
  import type { BgmSlot, Character, Scene } from '../../../types/index.js';
  import { getNormalizedBgmSlots } from '../../../utils/sceneSettings.js';
  import {
    applyMenuPosition,
    getMenuPositionStyle,
  } from '../../../utils/menuPosition.js';
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

  let isDeleteConfirmOpen = $state(false);
  let isDeleteLogsConfirmOpen = $state(false);
  let hoveredAction = $state<'create' | 'delete-logs' | 'delete-scene' | null>(null);

  // Per-character member move target names: characterId -> sceneName
  function initializeMemberTargets(): Record<string, string> {
    const targets: Record<string, string> = {};
    for (const char of characters) {
      const found = allScenes.find((s) => s.members.includes(char.id));
      targets[char.id] = found ? found.name : currentScene.name;
    }
    return targets;
  }

  let memberTargetSceneNames = $state<Record<string, string>>(initializeMemberTargets());
  let activeMemberSplitId = $state<string | null>(null);
  let membersMoveListContainer = $state<HTMLElement | null>(null);
  let toggleButtons = $state<Record<string, HTMLElement | undefined>>({});
  let menuElements = $state<Record<string, HTMLElement | undefined>>({});

  $effect(() => {
    if (
      activeMemberSplitId &&
      menuElements[activeMemberSplitId] &&
      toggleButtons[activeMemberSplitId]
    ) {
      return applyMenuPosition(
        menuElements[activeMemberSplitId],
        toggleButtons[activeMemberSplitId],
        {
          placement: 'bottom-end',
          anchorName: `--member-scene-anchor-${activeMemberSplitId}`,
        }
      );
    }
  });

  $effect(() => {
    for (const char of characters) {
      if (memberTargetSceneNames[char.id] === undefined) {
        const found = allScenes.find((s) => s.members.includes(char.id));
        memberTargetSceneNames[char.id] = found ? found.name : currentScene.name;
      }
    }
  });

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

  // BGM Slots state & synchronization (Issue #53)
  let bgmSlots = $state<BgmSlot[]>(getNormalizedBgmSlots(currentScene.juices));
  let activeBgmSlotPicking = $state<number | null>(null);
  let lastTinIds = $state<Record<number, string>>({});

  $effect(() => {
    bgmSlots = getNormalizedBgmSlots(currentScene.juices);
  });

  function syncSlotToLegacy(slot: BgmSlot) {
    if (slot.slot === 1) {
      currentScene.juices.bgm1 = slot.tinId ?? undefined;
      currentScene.juices.bgm1Volume = slot.volume;
    } else if (slot.slot === 2) {
      currentScene.juices.bgm2 = slot.tinId ?? undefined;
      currentScene.juices.bgm2Volume = slot.volume;
    } else if (slot.slot === 3) {
      currentScene.juices.bgm3 = slot.tinId ?? undefined;
      currentScene.juices.bgm3Volume = slot.volume;
    }
  }

  function handleToggleBgmPlay(slotNum: number) {
    const slot = bgmSlots.find((s) => s.slot === slotNum);
    if (!slot) return;

    if (slot.isPlaying && slot.tinId) {
      // 再生中 -> 停止 (None / null)
      lastTinIds[slotNum] = slot.tinId;
      bgmSlots = bgmSlots.map((s) =>
        s.slot === slotNum ? { ...s, tinId: null, isPlaying: false } : s,
      );
    } else {
      // 停止中 -> 再生
      const prev =
        lastTinIds[slotNum] ||
        (slotNum === 1
          ? currentScene.juices.bgm1
          : slotNum === 2
            ? currentScene.juices.bgm2
            : slotNum === 3
              ? currentScene.juices.bgm3
              : undefined);
      if (prev) {
        bgmSlots = bgmSlots.map((s) =>
          s.slot === slotNum ? { ...s, tinId: prev, isPlaying: true } : s,
        );
      } else {
        // 音源がなければアセット選択を開く
        handleBgmSelect(slotNum);
        return;
      }
    }

    const updated = bgmSlots.find((s) => s.slot === slotNum)!;
    syncSlotToLegacy(updated);
    currentScene.juices.bgmSlots = [...bgmSlots];
    onUpdateScene?.(currentScene);
  }

  function handleBgmSelect(slotOrKey: number | 'bgm1' | 'bgm2' | 'bgm3') {
    const slotNum =
      typeof slotOrKey === 'number'
        ? slotOrKey
        : slotOrKey === 'bgm1'
          ? 1
          : slotOrKey === 'bgm2'
            ? 2
            : 3;
    activeBgmSlotPicking = slotNum;
    assetPickerTarget = `bgm${slotNum}` as any;
    assetPickerMediaType = 'audio';
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'audio',
        initialScope: 'personal',
        onSelect: (dataUrl) => {
          handleSlotAssetPicked(slotNum, dataUrl);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  function handleSlotAssetPicked(slotNum: number, dataUrl: string | undefined) {
    if (dataUrl) {
      lastTinIds[slotNum] = dataUrl;
    }
    bgmSlots = bgmSlots.map((s) =>
      s.slot === slotNum
        ? { ...s, tinId: dataUrl ?? null, isPlaying: Boolean(dataUrl) }
        : s,
    );
    const updated = bgmSlots.find((s) => s.slot === slotNum);
    if (updated) {
      syncSlotToLegacy(updated);
    }
    currentScene.juices.bgmSlots = [...bgmSlots];
    onUpdateScene?.(currentScene);
  }

  function handleSlotVolumeInput(slotNum: number, volumeHundred: number) {
    const vol = Math.max(0, Math.min(100, volumeHundred)) / 100;
    bgmSlots = bgmSlots.map((s) => (s.slot === slotNum ? { ...s, volume: vol } : s));
    const updated = bgmSlots.find((s) => s.slot === slotNum);
    if (updated) {
      syncSlotToLegacy(updated);
    }
    currentScene.juices.bgmSlots = [...bgmSlots];
    onUpdateScene?.(currentScene);
  }

  function handleAddBgmSlot() {
    const maxSlot = bgmSlots.reduce((m, s) => Math.max(m, s.slot), 0);
    const newSlotNum = (maxSlot || 0) + 1;
    const newSlot: BgmSlot = {
      slot: newSlotNum,
      tinId: null,
      volume: 0.8,
      isPlaying: false,
    };
    bgmSlots = [...bgmSlots, newSlot];
    currentScene.juices.bgmSlots = [...bgmSlots];
    onUpdateScene?.(currentScene);
  }

  function handleAssetPicked(dataUrl: string | undefined) {
    if (assetPickerTarget === 'bgImage') {
      currentScene.juices.bgImage = dataUrl;
    } else if (activeBgmSlotPicking !== null) {
      handleSlotAssetPicked(activeBgmSlotPicking, dataUrl);
      activeBgmSlotPicking = null;
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

  function handleMoveMemberApply(charId: string) {
    const targetName =
      memberTargetSceneNames[charId] ||
      allScenes.find((s) => s.members.includes(charId))?.name ||
      currentScene.name;
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

  function handleWindowClick(e: MouseEvent) {
    if (!activeMemberSplitId) return;
    const target = e.target;
    if (target instanceof Node) {
      const activeToggle = toggleButtons[activeMemberSplitId];
      const activeMenu = menuElements[activeMemberSplitId];
      if (activeToggle?.contains(target) || activeMenu?.contains(target)) {
        return;
      }
    }
    activeMemberSplitId = null;
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="scene-settings-container" data-testid="scene-settings-tab">
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

    <!-- BGM Controls (Issue #53: Reconstructed with slot u32, toggle play/stop, narrow button, 1:1 select button, volume slider) -->
    <div class="bgm-controls" data-testid="bgm-controls">
      <div class="bgm-section-header">
        <span class="bgm-header-label">BGMスロット設定</span>
        <button
          type="button"
          class="add-slot-text-btn"
          onclick={handleAddBgmSlot}
          data-testid="add-bgm-slot-btn"
          aria-label="スロットを追加"
        >
          <Icon>add</Icon>
          <span>スロット追加</span>
        </button>
      </div>

      {#each bgmSlots as slot (slot.slot)}
        <div class="bgm-row" data-testid="bgm-row-{slot.slot}">
          <!-- 1. 停止/再生ボタン: M3 IconButton, width="narrow", toggle play/stop -->
          <IconButton
            variant="tonal"
            size="s"
            width="narrow"
            shape="round"
            aria-label={slot.isPlaying && slot.tinId ? 'BGM停止' : 'BGM再生'}
            onclick={() => handleToggleBgmPlay(slot.slot)}
            data-testid="bgm-play-btn-{slot.slot}"
            class="bgm-play-btn narrow-btn"
            title={slot.isPlaying && slot.tinId ? `スロット${slot.slot}: 停止` : `スロット${slot.slot}: 再生`}
          >
            <Icon>{slot.isPlaying && slot.tinId ? 'stop' : 'play_arrow'}</Icon>
          </IconButton>

          <!-- 2. 選択ボタン: 縦横比 1:1 の M3 IconButton, library_music アイコン -->
          <IconButton
            variant="tonal"
            size="s"
            width="default"
            shape="round"
            aria-label="BGM選択"
            onclick={() => handleBgmSelect(slot.slot)}
            data-testid="bgm-select-btn-{slot.slot}"
            class="bgm-select-btn square-btn"
            title={`スロット${slot.slot}: BGM選択`}
          >
            <Icon>library_music</Icon>
          </IconButton>

          <!-- 3. 音量バー: M3 Slider (0〜100) -->
          <div class="slider-wrapper" data-testid="bgm-slider-wrapper-{slot.slot}">
            <Slider
              min={0}
              max={100}
              step={1}
              value={Math.round((slot.volume ?? 0.8) * 100)}
              oninput={(e: Event) => {
                const target = e.target as HTMLInputElement;
                const val = (e as any).detail?.value ?? Number(target.value);
                handleSlotVolumeInput(slot.slot, val);
              }}
              onchange={(e: Event) => {
                const target = e.target as HTMLInputElement;
                const val = (e as any).detail?.value ?? Number(target.value);
                handleSlotVolumeInput(slot.slot, val);
              }}
              aria-label="BGM音量"
              data-testid="bgm-slider-{slot.slot}"
            />
            <span class="vol-label" data-testid="bgm-vol-label-{slot.slot}">
              {Math.round((slot.volume ?? 0.8) * 100)}%
            </span>
          </div>

          <!-- スロット番号バッジ (u32) -->
          <span class="slot-badge" data-testid="bgm-slot-badge-{slot.slot}">
            #{slot.slot}
          </span>
        </div>
      {/each}
    </div>

    <!-- Heading 管理 -->
    <h3 class="section-heading" data-testid="heading-management">管理</h3>

    <!-- Management buttons in 1 row with 7 : 5 : 3 ratio and soft Material Color for delete -->
    <div class="management-buttons" data-testid="management-buttons">
      <!-- Scene Name Edit Field (Issue #123: Moved above create scene in management section) -->
      <div class="scene-name-container" data-testid="scene-name-container">
        <TextField
          variant="outlined"
          label="シーン名"
          value={currentScene.name}
          oninput={handleSceneNameInput}
          data-testid="scene-name-input"
          placeholder="シーン名"
          class="scene-name-field"
        />
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
            size="m"
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
            size="m"
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
            size="m"
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

      <!-- メンバーを移動: Material 3 Expressive Segmented List & Menu (Issue #110) -->
      <div class="move-members-section" data-testid="move-members-section">
        <div class="move-members-header">
          <div class="move-members-title-group">
            <Icon>group</Icon>
            <span class="move-members-title" data-testid="move-members-title">メンバーを移動</span>
          </div>
          <span class="member-count-badge" data-testid="member-count-badge">
            {characters.length}人
          </span>
        </div>

        {#if characters.length === 0}
          <div class="empty-members-message" data-testid="empty-members-message">
            部屋にキャラクターがいません
          </div>
        {:else}
          <List
            class="members-move-list members-segmented-list"
            bind:element={membersMoveListContainer}
            data-testid="members-move-list"
          >
            {#each characters as char (char.id)}
              {@const currentSc = allScenes.find((s) => s.members.includes(char.id))}
              <ListItem
                type="text"
                class="member-move-row member-segmented-item"
                data-testid="move-row-{char.id}"
              >
                {#snippet avatar()}
                  <img
                    class="member-avatar"
                    src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                    alt={char.name}
                    data-testid="member-avatar-{char.id}"
                  />
                {/snippet}

                <span class="member-name" data-testid="member-name-{char.id}">{char.name}</span>

                {#snippet supportingText()}
                  <span class="member-current-scene" data-testid="member-current-scene-{char.id}">
                    {currentSc ? `現在: ${currentSc.name}` : '未所属'}
                  </span>
                {/snippet}

                {#snippet end()}
                  <div class="member-move-controls" onclick={(e) => e.stopPropagation()}>
                    <div class="split-select-wrapper">
                      <TextField
                        variant="outlined"
                        label="移動先"
                        placeholder="シーン名"
                        bind:value={memberTargetSceneNames[char.id]}
                        data-testid="member-target-input-{char.id}"
                        class="member-target-field"
                      />
                      <IconButton
                        variant="tonal"
                        size="s"
                        onclick={(e) => {
                          e.stopPropagation();
                          activeMemberSplitId =
                            activeMemberSplitId === char.id ? null : char.id;
                        }}
                        bind:element={toggleButtons[char.id]}
                        data-testid="member-split-toggle-{char.id}"
                        aria-label="移動先シーン候補を選択"
                        title="シーン一覧"
                        class="member-scene-toggle-btn"
                      >
                        <Icon>arrow_drop_down</Icon>
                      </IconButton>

                      <Menu
                        bind:element={menuElements[char.id]}
                        open={activeMemberSplitId === char.id}
                        anchor={toggleButtons[char.id]}
                        style={getMenuPositionStyle(toggleButtons[char.id], {
                          placement: 'bottom-end',
                          anchorName: `--member-scene-anchor-${char.id}`,
                        })}
                        data-testid="member-scene-menu-{char.id}"
                        class="member-scene-menu"
                        ontoggle={(e) => {
                          if (!e.newState || e.newState === 'closed') {
                            if (activeMemberSplitId === char.id) activeMemberSplitId = null;
                          }
                        }}
                      >
                        {#each allScenes as sc (sc.id)}
                          <MenuItem
                            selected={sc.name === memberTargetSceneNames[char.id]}
                            onclick={() => {
                              memberTargetSceneNames[char.id] = sc.name;
                              activeMemberSplitId = null;
                            }}
                            data-testid="member-scene-item-{char.id}-{sc.id}"
                          >
                            {sc.name}
                          </MenuItem>
                        {/each}
                      </Menu>
                    </div>

                    <Button
                      variant="tonal"
                      size="m"
                      class="apply-move-btn"
                      onclick={() => handleMoveMemberApply(char.id)}
                      data-testid="apply-move-btn-{char.id}"
                    >
                      移動
                    </Button>
                  </div>
                {/snippet}
              </ListItem>
            {/each}
          </List>
        {/if}
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
  .scene-name-container {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .scene-name-container :global(.scene-name-field) {
    width: 100%;
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
  .bgm-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
  }
  .bgm-header-label {
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
  }
  .add-slot-text-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--np-color-primary);
    cursor: pointer;
    padding: 4px 8px;
    border-radius: var(--np-shape-corner-small);
  }
  .add-slot-text-btn:hover {
    background: var(--np-color-surface-container-highest);
  }
  .bgm-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  /* 停止/再生ボタン: 選択ボタン（1:1）に対してより細い幅 (narrow) */
  :global(.bgm-play-btn.narrow-btn) {
    width: 2rem !important;
    min-width: 2rem !important;
    max-width: 2rem !important;
    height: 2.5rem !important;
    flex-shrink: 0;
  }
  /* 選択ボタン: 縦横比 1:1 */
  :global(.bgm-select-btn.square-btn) {
    width: 2.5rem !important;
    min-width: 2.5rem !important;
    max-width: 2.5rem !important;
    height: 2.5rem !important;
    aspect-ratio: 1 / 1;
    flex-shrink: 0;
  }
  .slider-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .slider-wrapper :global(.np-slider) {
    flex: 1;
  }
  .vol-label {
    font-size: 0.8rem;
    width: 36px;
    text-align: right;
    color: var(--np-color-on-surface-variant);
    flex-shrink: 0;
  }
  .slot-badge {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--np-color-on-surface-variant);
    background: var(--np-color-surface-container-high);
    padding: 2px 6px;
    border-radius: var(--np-shape-corner-extra-small);
    flex-shrink: 0;
  }
  .management-buttons {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .move-members-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--np-color-surface-container);
    border: 1px solid var(--np-color-outline-variant);
    border-radius: var(--np-shape-corner-medium);
    padding: 12px;
  }
  .move-members-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--np-color-outline-variant);
  }
  .move-members-title-group {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--np-color-on-surface);
  }
  .move-members-title {
    font-size: 0.95rem;
    font-weight: 600;
  }
  .member-count-badge {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--np-color-on-surface-variant);
    background: var(--np-color-surface-container-high);
    padding: 2px 8px;
    border-radius: var(--np-shape-corner-full);
  }
  .empty-members-message {
    padding: 16px 8px;
    text-align: center;
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
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
  :global(.members-segmented-list) {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 340px;
    overflow-y: auto;
    padding: 0;
    margin: 0;
    list-style: none;
  }
  :global(.member-segmented-item.np-item) {
    display: grid !important;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    gap: 8px 12px;
    align-items: center;
    width: 100%;
    min-height: unset !important;
    padding: 12px !important;
    background: var(--np-color-surface-container-low);
    border: 1px solid var(--np-color-outline-variant);
    border-radius: var(--np-shape-corner-medium, 12px);
    box-sizing: border-box;
    transition: background-color var(--np-motion-expressive-fast-effects, 150ms ease),
                border-color var(--np-motion-expressive-fast-effects, 150ms ease);
    cursor: default !important;
  }
  :global(.member-segmented-item.np-item:hover) {
    background: var(--np-color-surface-container);
  }
  :global(.member-segmented-item .np-item-avatar) {
    grid-column: 1;
    grid-row: 1;
    width: 36px;
    height: 36px;
    border-radius: var(--np-shape-corner-full);
    border: 1px solid var(--np-color-outline-variant);
    overflow: hidden;
  }
  :global(.member-segmented-item .np-item-text) {
    grid-column: 2;
    grid-row: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  :global(.member-segmented-item .np-item-end) {
    grid-column: 1 / -1;
    grid-row: 2;
    width: 100%;
    display: flex;
  }
  .member-avatar {
    width: 100%;
    height: 100%;
    border-radius: var(--np-shape-corner-full);
    object-fit: cover;
    flex-shrink: 0;
  }
  .member-name {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .member-current-scene {
    font-size: 0.75rem;
    color: var(--np-color-on-surface-variant);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .member-move-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
  }
  .split-select-wrapper {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .split-select-wrapper :global(.member-target-field) {
    flex: 1;
    min-width: 0;
  }
  :global(.member-scene-toggle-btn) {
    flex-shrink: 0;
  }
  :global(.member-scene-menu) {
    max-height: 200px;
    overflow-y: auto;
  }
  :global(.apply-move-btn) {
    flex-shrink: 0;
    white-space: nowrap;
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
