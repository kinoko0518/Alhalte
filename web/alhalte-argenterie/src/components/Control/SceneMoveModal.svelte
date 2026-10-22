<script lang="ts">
  /**
   * 「キャラクターを移動」 modal (Issue #269).
   *
   * Root-hoisted full-screen dialog (the parent `App` renders it outside the
   * control panel, like `CharacterEditModal`) that moves one character to an
   * existing scene or to a scene created from the typed name.
   *
   * The scene selection is the shared `SceneSwitcher`: Enter in its search field
   * and the filled 「移動」 button both call `switcher.resolve()`, so the two
   * paths cannot diverge. The modal owns no state besides the in-flight flag; the
   * scene list, the created scene and the character move are acknowledged by the
   * server through `App` and reported back as `false` on failure.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### scene の管理操作`
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### キャラクター同期`
   * - GitHub Issue #269: シーンの移動からシーンを新規作成する導線を作る
   */
  import { Button } from 'noph-ui';
  import type { Character, Scene } from '../../types/index.js';
  import SceneSwitcher from './SceneSwitcher.svelte';

  let {
    character,
    scenes = [],
    characterSceneId = null,
    canCreate = false,
    onSelectScene,
    onCreateScene,
    onClose,
  }: {
    /** Character being moved (`App.activeMoveCharacter`). */
    character: Character;
    /** Every scene of the room, including the character's current one. */
    scenes?: Scene[];
    /** Scene that currently holds the character (rendered as selected). */
    characterSceneId?: string | null;
    /** Whether an unknown name may create a scene (admin-only). */
    canCreate?: boolean;
    /** Moves the character to an existing scene; `false` = failure. */
    onSelectScene?: (sceneId: string) => void | boolean | Promise<void | boolean>;
    /** Creates the scene then moves the character; `false` = failure. */
    onCreateScene?: (
      name: string,
      sceneId: string,
    ) => void | boolean | Promise<void | boolean>;
    onClose?: () => void;
  } = $props();

  let switcher = $state<ReturnType<typeof SceneSwitcher> | null>(null);
  /** The 「移動」 button is waiting for the switcher's resolve(). */
  let isSubmitting = $state(false);

  /**
   * 「移動」 button: exactly the Enter path (`resolve()`) of the switcher.
   *
   * The parent closes the modal after a successful acknowledgement; a failure
   * leaves it open with the switcher's own error text.
   */
  async function handleConfirm(): Promise<void> {
    if (isSubmitting) return;
    isSubmitting = true;
    try {
      await switcher?.resolve();
    } finally {
      isSubmitting = false;
    }
  }

  function handleBackdropClick(event: MouseEvent): void {
    if (event.target !== event.currentTarget) return;
    if (isSubmitting) return;
    onClose?.();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="scene-move-overlay"
  data-testid="move-character-modal"
  role="dialog"
  aria-modal="true"
  aria-labelledby="scene-move-title"
  tabindex="-1"
  onclick={handleBackdropClick}
>
  <div class="scene-move-dialog">
    <h2 class="scene-move-title" id="scene-move-title" data-testid="move-character-modal-title">
      キャラクターを移動
    </h2>
    <p class="scene-move-description" data-testid="move-character-modal-description">
      「{character.name}」の移動先を選んでください。
    </p>

    <SceneSwitcher
      bind:this={switcher}
      {scenes}
      currentSceneId={characterSceneId}
      {canCreate}
      testIdPrefix="move-scene-search"
      emptyNoticeTestId="move-scene-search-empty-notice"
      placeholder="移動先のシーン"
      ariaLabel="移動先のシーンを検索・選択"
      searchClass="scene-move-search"
      {onSelectScene}
      {onCreateScene}
    />

    <div class="scene-move-actions">
      <Button
        variant="text"
        onclick={() => onClose?.()}
        disabled={isSubmitting}
        data-testid="move-character-cancel"
      >
        キャンセル
      </Button>
      <Button
        variant="filled"
        onclick={handleConfirm}
        disabled={isSubmitting}
        data-testid="move-character-confirm"
      >
        移動
      </Button>
    </div>
  </div>
</div>

<style>
  .scene-move-overlay {
    position: fixed;
    inset: 0;
    z-index: 9000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    box-sizing: border-box;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
  }
  .scene-move-dialog {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 90%;
    max-width: 480px;
    max-height: 85vh;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 24px;
    box-sizing: border-box;
    background: var(--np-color-surface-container-high, #ece6f0);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    box-shadow: var(--np-elevation-3, 0 8px 24px rgba(0, 0, 0, 0.2));
  }
  .scene-move-title {
    margin: 0;
    text-align: center;
    font-size: 1.25rem;
    font-weight: 600;
    color: var(--np-color-on-surface, #1d1b20);
  }
  .scene-move-description {
    margin: 0;
    text-align: center;
    font-size: 0.9rem;
    color: var(--np-color-on-surface-variant, #49454f);
    line-height: 1.4;
  }
  .scene-move-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  /* The modal owns the switcher's width; the header-only size overrides must
     not apply here (`.common-scene-search` is not used). */
  :global(.scene-move-search) {
    --_bar-height: 3rem;
    --_pane-margin: 0px;
    width: 100%;
  }
</style>
