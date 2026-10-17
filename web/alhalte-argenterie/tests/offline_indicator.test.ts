/**
 * Unit and integration tests for Server Connection Management & Offline Indicator.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie`
 * - GitHub Issue #67: `feat(argenterie): サーバー接続状態の明示とオフライン検出インジケーターの実装`
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import OfflineIndicator from '../src/components/OfflineIndicator.svelte';
import {
  ConnectionManager,
  createConnectionManager,
  type ConnectionState,
} from '../src/utils/connection.js';
import type { VisitedRoom } from '../src/types/index.js';

describe('Server Connection Management & Offline Indicator (Issue #67)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
    document.body.innerHTML = '';
  });

  const dummyRoom: VisitedRoom = {
    id: 'room-offline-test',
    name: 'オフラインテスト部屋',
    url: 'https://alhalte.example/rooms/room-offline-test#dummy-key',
    lastVisited: Date.now(),
  };

  // spec: alhalte-argenterie/connection ConnectionManager store and lifecycle
  describe('ConnectionManager Unit Tests', () => {
    it('manages connection states and notifies subscribers via Svelte store contract', () => {
      const manager = createConnectionManager({ initialState: 'connected' });
      expect(manager.state).toBe('connected');
      expect(manager.isConnected).toBe(true);
      expect(manager.isOffline).toBe(false);

      const states: ConnectionState[] = [];
      const unsubscribe = manager.subscribe((s) => states.push(s));

      expect(states).toEqual(['connected']);

      manager.setState('disconnected');
      expect(manager.state).toBe('disconnected');
      expect(manager.isOffline).toBe(true);
      expect(manager.isConnected).toBe(false);

      manager.setState('reconnecting');
      expect(manager.state).toBe('reconnecting');
      expect(manager.isOffline).toBe(true);
      expect(manager.isConnecting).toBe(true);

      manager.setState('connected');
      expect(manager.isConnected).toBe(true);

      unsubscribe();
      manager.setState('disconnected');
      expect(states).toEqual(['connected', 'disconnected', 'reconnecting', 'connected']);
    });

    it('triggers reconnect handler on reconnect() and handles success and error', async () => {
      const mockHandler = vi.fn().mockResolvedValue(undefined);
      const manager = new ConnectionManager({
        initialState: 'disconnected',
        reconnectHandler: mockHandler,
      });

      expect(manager.state).toBe('disconnected');
      await manager.reconnect();

      expect(mockHandler).toHaveBeenCalledTimes(1);
      expect(manager.state).toBe('connected');

      // Failure case
      const failingHandler = vi.fn().mockRejectedValue(new Error('Network error'));
      manager.setReconnectHandler(failingHandler);

      await expect(manager.reconnect()).rejects.toThrow('Network error');
      expect(manager.state).toBe('disconnected');
    });

    it('handles browser online and offline events when enabled', () => {
      const manager = new ConnectionManager({
        initialState: 'connected',
        autoDetectBrowserStatus: true,
      });

      window.dispatchEvent(new Event('offline'));
      expect(manager.state).toBe('disconnected');

      window.dispatchEvent(new Event('online'));
      expect(manager.state).toBe('connected');

      manager.destroy();
    });
  });

  // spec: alhalte-argenterie/offline-indicator Standalone Component Tests
  describe('OfflineIndicator Component', () => {
    it('does not render when state is connected and debugAlwaysShow is false', () => {
      const instance = mount(OfflineIndicator, {
        target,
        props: {
          connectionState: 'connected',
          debugMode: false,
          debugAlwaysShow: false,
        },
      });

      const indicator = target.querySelector('[data-testid="offline-indicator"]');
      expect(indicator).toBeNull();

      unmount(instance);
    });

    it('renders prominent banner with error tone, cloud_off icon, and reconnect button when disconnected', async () => {
      const onReconnect = vi.fn();
      const instance = mount(OfflineIndicator, {
        target,
        props: {
          connectionState: 'disconnected',
          debugMode: false,
          onReconnect,
        },
      });

      const indicator = target.querySelector<HTMLElement>('[data-testid="offline-indicator"]');
      expect(indicator).toBeTruthy();
      expect(indicator?.getAttribute('data-state')).toBe('disconnected');

      // Message check
      const messageEl = target.querySelector('[data-testid="offline-message"]');
      expect(messageEl?.textContent?.trim()).toContain('サーバー未接続 (オフライン動作中)');

      // Icon check
      const iconEl = target.querySelector('[data-testid="offline-indicator-icon"]');
      expect(iconEl?.textContent?.trim()).toContain('cloud_off');

      // Reconnect button
      const reconnectBtn = target.querySelector<HTMLButtonElement>('[data-testid="reconnect-button"]');
      expect(reconnectBtn).toBeTruthy();
      expect(reconnectBtn?.textContent).toContain('再接続');
      expect(reconnectBtn?.textContent).toContain('refresh');

      reconnectBtn?.click();
      expect(onReconnect).toHaveBeenCalledTimes(1);

      unmount(instance);
    });

    it('renders reconnecting state with disabled button and spinning indicator', () => {
      const instance = mount(OfflineIndicator, {
        target,
        props: {
          connectionState: 'reconnecting',
          debugMode: false,
        },
      });

      const indicator = target.querySelector<HTMLElement>('[data-testid="offline-indicator"]');
      expect(indicator).toBeTruthy();
      expect(indicator?.classList.contains('is-reconnecting')).toBe(true);

      const messageEl = target.querySelector('[data-testid="offline-message"]');
      expect(messageEl?.textContent).toContain('サーバー未接続 (オフライン動作中)');
      expect(messageEl?.textContent).toContain('再接続中');

      const reconnectBtn = target.querySelector<HTMLButtonElement>('[data-testid="reconnect-button"]');
      expect(reconnectBtn?.disabled).toBe(true);

      unmount(instance);
    });
  });

  // spec: alhalte-argenterie/app-offline App Integration Tests (Issue #67 requirements)
  describe('App Offline Detection Integration', () => {
    // Requirement: 接続状態の変化（connected -> disconnected -> reconnecting）に応じてオフラインインジケーターが表示・非表示となること
    it('shows and hides the offline indicator dynamically as connection state changes', async () => {
      const manager = createConnectionManager({ initialState: 'connected' });

      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          connectionManager: manager,
          debugMode: false,
        },
      });

      await new Promise((r) => setTimeout(r, 20));

      // 1. connected -> indicator is hidden
      expect(target.querySelector('[data-testid="offline-indicator"]')).toBeNull();

      // 2. disconnected -> indicator is displayed prominently
      manager.setState('disconnected');
      await new Promise((r) => setTimeout(r, 20));

      const offlineBanner = target.querySelector<HTMLElement>('[data-testid="offline-indicator"]');
      expect(offlineBanner).toBeTruthy();
      expect(offlineBanner?.getAttribute('data-state')).toBe('disconnected');
      expect(offlineBanner?.textContent).toContain('サーバー未接続 (オフライン動作中)');

      // 3. reconnecting -> indicator remains displayed with reconnecting tone
      manager.setState('reconnecting');
      await new Promise((r) => setTimeout(r, 20));

      const reconnectingBanner = target.querySelector<HTMLElement>('[data-testid="offline-indicator"]');
      expect(reconnectingBanner).toBeTruthy();
      expect(reconnectingBanner?.getAttribute('data-state')).toBe('reconnecting');

      // 4. connected -> indicator is hidden again
      manager.setState('connected');
      await new Promise((r) => setTimeout(r, 20));

      expect(target.querySelector('[data-testid="offline-indicator"]')).toBeNull();

      unmount(instance);
    });

    // Requirement: 再接続ボタンの押下で再接続ハンドラが呼び出されること
    it('invokes reconnect handler when reconnect button is clicked', async () => {
      const mockReconnect = vi.fn().mockImplementation(async () => {
        // simulate async reconnection
        await new Promise((r) => setTimeout(r, 10));
      });

      const manager = createConnectionManager({
        initialState: 'disconnected',
        reconnectHandler: mockReconnect,
      });

      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          connectionManager: manager,
          debugMode: false,
        },
      });

      await new Promise((r) => setTimeout(r, 20));

      const reconnectBtn = target.querySelector<HTMLButtonElement>('[data-testid="reconnect-button"]');
      expect(reconnectBtn).toBeTruthy();

      reconnectBtn?.click();
      await new Promise((r) => setTimeout(r, 40));

      expect(mockReconnect).toHaveBeenCalledTimes(1);
      // After successful reconnection, indicator should be hidden
      expect(target.querySelector('[data-testid="offline-indicator"]')).toBeNull();

      unmount(instance);
    });

    // Requirement: デバッグモード下でオフライン状態が明示的にUI上に現れること
    it('explicitly displays offline status with debug badge in debug mode', async () => {
      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          connectionState: 'disconnected',
          debugMode: true,
        },
      });

      await new Promise((r) => setTimeout(r, 20));

      const indicator = target.querySelector<HTMLElement>('[data-testid="offline-indicator"]');
      expect(indicator).toBeTruthy();
      expect(indicator?.getAttribute('data-debug')).toBe('true');
      expect(indicator?.classList.contains('debug-mode')).toBe(true);

      // Verify debug badge explicitly shows state
      const debugBadge = target.querySelector('[data-testid="debug-badge"]');
      expect(debugBadge).toBeTruthy();
      expect(debugBadge?.textContent?.trim()).toBe('DEV: disconnected');

      unmount(instance);
    });

    // Verify offline indicator also functions on HomeScreen (initialRoom = null)
    it('displays offline indicator at frontmost viewport level on HomeScreen', async () => {
      const instance = mount(App, {
        target,
        props: {
          initialRoom: null,
          connectionState: 'disconnected',
          debugMode: false,
        },
      });

      await new Promise((r) => setTimeout(r, 20));

      // Home screen is visible
      expect(target.querySelector('[data-testid="home-screen"]')).toBeTruthy();

      // Offline indicator is also visible at top of viewport
      const indicator = target.querySelector('[data-testid="offline-indicator"]');
      expect(indicator).toBeTruthy();
      expect(indicator?.textContent).toContain('サーバー未接続 (オフライン動作中)');

      unmount(instance);
    });
  });
});
