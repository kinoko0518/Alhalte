<script lang="ts">
  /**
   * Offline / Connection Status Indicator banner for Alhalte Argenterie.
   *
   * Adheres to Material 3 Expressive guidelines:
   * - Error tone (--np-color-error-container, --np-color-on-error-container) when disconnected
   * - Tertiary tone (--np-color-tertiary-container, --np-color-on-tertiary-container) when reconnecting
   * - Smooth slide-in transition from off-screen above the viewport
   * - Status icons: cloud_off (disconnected), sync with spin (reconnecting)
   * - Status messages: 「サーバーとの接続が切断されました」(disconnected), 「サーバーと再接続中です……」(reconnecting)
   * - Right-aligned M3 Button with "再接続" label
   * - Dotted/dashed borders removed
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie`
   * - GitHub Issue #88: `オフライン動作に関する提言`
   * - GitHub Issue #111: `オフライン動作であることを通知するバーが他のUIに覆い被さっている`
   */
  import { Button } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { ConnectionState } from '../utils/connection.js';

  let {
    connectionState = 'disconnected',
    debugMode = false,
    debugAlwaysShow = false,
    onReconnect,
  }: {
    connectionState?: ConnectionState;
    debugMode?: boolean;
    debugAlwaysShow?: boolean;
    onReconnect?: () => Promise<void> | void;
  } = $props();

  let isOffline = $derived(
    debugAlwaysShow || connectionState === 'disconnected' || connectionState === 'reconnecting',
  );
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

<aside
  class="offline-indicator-banner"
  class:is-visible={isOffline}
  class:is-reconnecting={isReconnecting}
  data-testid="offline-indicator"
  data-state={connectionState}
  role="status"
  aria-live="polite"
  aria-hidden={!isOffline}
>
  <div class="indicator-inner">
    <div class="indicator-main">
      <div class="indicator-icon" data-testid="offline-indicator-icon">
        {#if isReconnecting}
          <Icon class="spin-icon">sync</Icon>
        {:else}
          <Icon>cloud_off</Icon>
        {/if}
      </div>

      <div class="indicator-content">
        <span class="indicator-message" data-testid="offline-message">
          {#if isReconnecting}
            サーバーと再接続中です……
          {:else}
            サーバーとの接続が切断されました
          {/if}
        </span>
      </div>
    </div>

    <div class="indicator-actions">
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
    </div>
  </div>
</aside>

<style>
  .offline-indicator-banner {
    position: relative;
    width: 100%;
    display: grid;
    grid-template-rows: 0fr;
    max-height: 0;
    overflow: hidden;
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    box-sizing: border-box;
    background-color: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    border-bottom: 0 solid transparent;
    box-shadow: none;
    flex-shrink: 0;
    transition:
      grid-template-rows var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
      max-height var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
      opacity var(--np-motion-expressive-fast-effects, 200ms ease),
      visibility var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1)),
      background-color var(--np-motion-expressive-fast-effects, 200ms ease),
      color var(--np-motion-expressive-fast-effects, 200ms ease),
      border-bottom-color var(--np-motion-expressive-fast-effects, 200ms ease),
      box-shadow var(--np-motion-expressive-fast-effects, 200ms ease);
    z-index: 100;
  }

  .offline-indicator-banner.is-visible {
    grid-template-rows: 1fr;
    max-height: 160px;
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    border-bottom: 1px solid var(--np-color-error);
    box-shadow: var(--np-elevation-2);
  }

  .offline-indicator-banner.is-reconnecting {
    background-color: var(--np-color-tertiary-container);
    color: var(--np-color-on-tertiary-container);
  }

  .offline-indicator-banner.is-reconnecting.is-visible {
    border-bottom: 1px solid var(--np-color-tertiary);
  }

  .indicator-inner {
    overflow: hidden;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    gap: 12px;
    box-sizing: border-box;
    transition: padding var(--np-motion-expressive-default-spatial, 350ms cubic-bezier(0.2, 0, 0, 1));
  }

  .offline-indicator-banner.is-visible .indicator-inner {
    padding: 8px 16px;
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

  .indicator-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    margin-left: auto;
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
