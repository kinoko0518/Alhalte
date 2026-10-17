<script lang="ts">
  /**
   * Offline / Connection Status Indicator component for Alhalte Argenterie.
   *
   * Adheres to Material 3 Expressive guidelines:
   * - Error / Destructive / Warning tone tokens (--np-color-error-container, --np-color-on-error-container)
   * - M3 Button / IconButton with refresh icon and "再接続" label
   * - Google Material Symbols Outlined Icon (cloud_off)
   * - Explicit debug indicator on DEV mode to avoid silent failures
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie`
   * - GitHub Issue #67: `feat(argenterie): サーバー接続状態の明示とオフライン検出インジケーターの実装`
   */
  import { Button } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { ConnectionState } from '../utils/connection.js';

  let {
    connectionState = 'disconnected',
    debugMode,
    debugAlwaysShow = false,
    onReconnect,
  }: {
    connectionState?: ConnectionState;
    debugMode?: boolean;
    debugAlwaysShow?: boolean;
    onReconnect?: () => Promise<void> | void;
  } = $props();

  const isDev = typeof import.meta !== 'undefined' && Boolean(import.meta.env?.DEV);
  let effectiveDebug = $derived(debugMode !== undefined ? debugMode : isDev);

  let isOffline = $derived(connectionState === 'disconnected' || connectionState === 'reconnecting');
  let isVisible = $derived(debugAlwaysShow ? true : isOffline);
  let isReconnecting = $derived(connectionState === 'reconnecting');
  let isConnected = $derived(connectionState === 'connected');

  let isRetrying = $state(false);

  async function handleReconnect() {
    if (isReconnecting || isRetrying) return;
    if (onReconnect) {
      try {
        isRetrying = true;
        await onReconnect();
      } finally {
        isRetrying = false;
      }
    }
  }
</script>

{#snippet refreshIcon()}
  <Icon class={isReconnecting || isRetrying ? 'spin-icon' : ''}>refresh</Icon>
{/snippet}

{#if isVisible}
  <aside
    class="offline-indicator-banner"
    class:is-reconnecting={isReconnecting}
    class:is-connected={isConnected}
    class:debug-mode={effectiveDebug}
    data-testid="offline-indicator"
    data-state={connectionState}
    data-debug={effectiveDebug ? 'true' : 'false'}
    role="status"
    aria-live="polite"
  >
    <div class="indicator-main">
      <div class="indicator-icon" data-testid="offline-indicator-icon">
        {#if isConnected}
          <Icon>cloud_done</Icon>
        {:else if isReconnecting}
          <Icon class="spin-icon">sync</Icon>
        {:else}
          <Icon>cloud_off</Icon>
        {/if}
      </div>

      <div class="indicator-content">
        <span class="indicator-message" data-testid="offline-message">
          {#if isConnected}
            サーバー接続中
          {:else if isReconnecting}
            サーバー未接続 (オフライン動作中) - 再接続中...
          {:else}
            サーバー未接続 (オフライン動作中)
          {/if}
        </span>
        {#if effectiveDebug}
          <span class="debug-badge" data-testid="debug-badge">
            DEV: {connectionState}
          </span>
        {/if}
      </div>
    </div>

    <div class="indicator-actions">
      {#if !isConnected}
        <Button
          variant="tonal"
          size="s"
          disabled={isReconnecting || isRetrying}
          start={refreshIcon}
          onclick={handleReconnect}
          data-testid="reconnect-button"
          class="reconnect-button"
        >
          再接続
        </Button>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .offline-indicator-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    gap: 12px;
    background-color: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    box-shadow: var(--np-elevation-2);
    border-bottom: 1px solid var(--np-color-error);
    transition:
      background-color var(--np-motion-expressive-fast-effects, 200ms ease),
      color var(--np-motion-expressive-fast-effects, 200ms ease);
    z-index: 1200;
    width: 100%;
    box-sizing: border-box;
  }

  .offline-indicator-banner.is-reconnecting {
    background-color: var(--np-color-tertiary-container);
    color: var(--np-color-on-tertiary-container);
    border-bottom: 1px solid var(--np-color-tertiary);
  }

  .offline-indicator-banner.is-connected {
    background-color: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    border-bottom: 1px solid var(--np-color-outline-variant);
  }

  .offline-indicator-banner.debug-mode {
    outline: 2px dashed var(--np-color-error);
    outline-offset: -2px;
  }

  .offline-indicator-banner.debug-mode.is-reconnecting {
    outline-color: var(--np-color-tertiary);
  }

  .offline-indicator-banner.debug-mode.is-connected {
    outline-color: var(--np-color-primary);
  }

  .indicator-main {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1;
  }

  .indicator-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    font-size: 1.25rem;
  }

  .indicator-content {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    min-width: 0;
  }

  .indicator-message {
    font-weight: 600;
    font-size: 0.9rem;
    line-height: 1.4;
  }

  .debug-badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--np-shape-corner-full, 9999px);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.5px;
    background-color: var(--np-color-error);
    color: var(--np-color-on-error);
  }

  .offline-indicator-banner.is-reconnecting .debug-badge {
    background-color: var(--np-color-tertiary);
    color: var(--np-color-on-tertiary);
  }

  .offline-indicator-banner.is-connected .debug-badge {
    background-color: var(--np-color-primary);
    color: var(--np-color-on-primary);
  }

  .indicator-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  :global(.offline-indicator-banner .reconnect-button) {
    --np-button-container-color: var(--np-color-error);
    --np-button-label-text-color: var(--np-color-on-error);
  }

  :global(.offline-indicator-banner.is-reconnecting .reconnect-button) {
    --np-button-container-color: var(--np-color-tertiary);
    --np-button-label-text-color: var(--np-color-on-tertiary);
  }

  :global(.spin-icon) {
    animation: np-spin 1s linear infinite;
  }

  @keyframes np-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
