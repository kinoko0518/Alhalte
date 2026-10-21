<script lang="ts">
  /**
   * OfflineQueueNotice — notice for work queued while the server is unreachable.
   *
   * Displays a Material 3 Expressive notice framed by CSS wavy dividers (no
   * images) that explains what happens to work created while the server is
   * unreachable:
   *
   * - `kind="message"`: 「メッセージは次回の接続時に送信されます」
   * - `kind="character"`: 「キャラクターは次回の接続時に更新されます」
   * - `kind="generic"`: the `description` prop, or a generic fallback
   *
   * `count` renders 「N 件を蓄積中」 when provided. The element is a polite live
   * region (`role="status"`, `aria-live="polite"`) so screen readers announce
   * queue changes without stealing focus.
   *
   * The notice reports queue state only. It does not render a global connection
   * bar; connection failures surface inside the tab or editor that performed the
   * operation (Issue #261).
   *
   * Styling uses only `--np-color-*` / `--np-shape-*` semantic tokens.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
   * - GitHub Issue #109 / sub-issue #137: `オフラインキュー通知 UI`
   */
  /** Kinds this notice knows how to describe. `state` / `asset` map to the generic copy. */
  type OfflineQueueNoticeKind = 'message' | 'character' | 'generic';

  const KIND_DESCRIPTIONS: Record<Exclude<OfflineQueueNoticeKind, 'generic'>, string> = {
    message: 'メッセージは次回の接続時に送信されます',
    character: 'キャラクターは次回の接続時に更新されます',
  };

  const GENERIC_DESCRIPTION = 'オフライン中の変更は次回の接続時に反映されます';

  let {
    kind = 'generic',
    description,
    count,
    class: className = '',
  }: {
    /**
     * Which kind of queued work is being described.
     * @default 'generic'
     */
    kind?: OfflineQueueNoticeKind;
    /**
     * Explanation used only when `kind="generic"`. Message and character kinds
     * always use their canonical copy.
     */
    description?: string;
    /**
     * Number of queued entries. When provided (including `0`), 「N 件を蓄積中」
     * is shown. Omit it to hide the counter.
     */
    count?: number;
    /** Optional extra class for layout placement by the parent. */
    class?: string;
  } = $props();

  let resolvedDescription = $derived(
    kind === 'generic' ? (description ?? GENERIC_DESCRIPTION) : KIND_DESCRIPTIONS[kind],
  );

  let hasCount = $derived(typeof count === 'number' && Number.isFinite(count));
</script>

<aside
  class="offline-queue-notice {className}"
  class:kind-message={kind === 'message'}
  class:kind-character={kind === 'character'}
  class:kind-generic={kind === 'generic'}
  data-testid="offline-queue-notice"
  data-kind={kind}
  role="status"
  aria-live="polite"
  aria-atomic="true"
>
  <div
    class="wavy-divider wavy-divider-top"
    data-testid="offline-queue-notice-divider"
    aria-hidden="true"
  ></div>

  <div class="notice-content">
    <p class="notice-title" data-testid="offline-queue-notice-title">
      サーバーと通信できませんでした
    </p>
    <p class="notice-description" data-testid="offline-queue-notice-description">
      {resolvedDescription}
    </p>
    {#if hasCount}
      <span class="notice-count" data-testid="offline-queue-notice-count">{count} 件を蓄積中</span>
    {/if}
  </div>

  <div class="wavy-divider wavy-divider-bottom" aria-hidden="true"></div>
</aside>

<style>
  .offline-queue-notice {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    width: 100%;
    box-sizing: border-box;
    overflow: hidden;
    color: var(--np-color-on-surface);
    background-color: var(--np-color-surface-container);
    border-radius: var(--np-shape-corner-large, 16px);
  }

  .notice-content {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 0 16px;
    text-align: start;
  }

  .notice-title {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 600;
    line-height: 1.4;
    color: var(--np-color-on-surface);
  }

  .notice-description {
    margin: 0;
    font-size: 0.82rem;
    line-height: 1.5;
    color: var(--np-color-on-surface-variant);
  }

  .notice-count {
    display: inline-flex;
    align-items: center;
    margin-block-start: 4px;
    padding: 2px 10px;
    border-radius: var(--np-shape-corner-full, 9999px);
    font-size: 0.75rem;
    font-weight: 500;
    line-height: 1.4;
    color: var(--np-color-on-secondary-container);
    background-color: var(--np-color-secondary-container);
  }

  /*
   * Wavy divider: a row of touching semicircular arcs drawn with a radial
   * gradient, repeated horizontally. No image assets and no hard-coded colors —
   * only semantic M3 tokens. background-size is 11px wide (2 x the outer radius)
   * so neighboring arcs meet at the baseline and read as one continuous wave.
   */
  .wavy-divider {
    flex: none;
    width: 100%;
    height: 9px;
    background-image: radial-gradient(
      circle at 50% 100%,
      transparent 0,
      transparent 4px,
      var(--np-color-outline-variant) 4px,
      var(--np-color-outline-variant) 5.5px,
      transparent 5.5px
    );
    background-repeat: repeat-x;
    background-position: center bottom;
    background-size: 11px 9px;
  }

  .wavy-divider-bottom {
    transform: scaleY(-1);
  }

  @media (prefers-reduced-transparency: reduce) {
    .offline-queue-notice {
      background-color: var(--np-color-surface-container-high);
    }
  }
</style>
