/**
 * Connection state manager and reactive store for server connectivity.
 *
 * Prevents silent fallback masking communication failures by reactively
 * tracking and exposing connection states.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie`
 * - GitHub Issue #67: `feat(argenterie): サーバー接続状態の明示とオフライン検出インジケーターの実装`
 */

/**
 * Server connection states.
 */
export type ConnectionState = 'connected' | 'connecting' | 'disconnected' | 'reconnecting';

export interface ConnectionManagerOptions {
  /**
   * Initial connection state (defaults to 'connected').
   */
  initialState?: ConnectionState;
  /**
   * Handler to invoke when reconnection is attempted.
   */
  reconnectHandler?: () => Promise<void> | void;
  /**
   * Whether to automatically attach window online/offline event listeners.
   */
  autoDetectBrowserStatus?: boolean;
}

/**
 * Manages server connection state and provides a reactive subscription model
 * conforming to the Svelte store contract.
 */
export class ConnectionManager {
  private _state: ConnectionState;
  private _listeners: Set<(state: ConnectionState) => void> = new Set();
  private _reconnectHandler?: () => Promise<void> | void;
  private _cleanupBrowserListeners?: () => void;

  constructor(options: ConnectionManagerOptions = {}) {
    this._state = options.initialState ?? 'connected';
    this._reconnectHandler = options.reconnectHandler;

    if (options.autoDetectBrowserStatus) {
      this.enableBrowserStatusDetection();
    }
  }

  /**
   * Current connection state.
   */
  get state(): ConnectionState {
    return this._state;
  }

  set state(newState: ConnectionState) {
    this.setState(newState);
  }

  /**
   * Get current connection state.
   */
  getState(): ConnectionState {
    return this._state;
  }

  /**
   * Set connection state and notify all subscribers.
   */
  setState(newState: ConnectionState): void {
    if (this._state === newState) return;
    this._state = newState;
    this.notify();
  }

  /**
   * Returns true if offline (disconnected or reconnecting).
   */
  get isOffline(): boolean {
    return this._state === 'disconnected' || this._state === 'reconnecting';
  }

  /**
   * Returns true if connected.
   */
  get isConnected(): boolean {
    return this._state === 'connected';
  }

  /**
   * Returns true if currently attempting connection.
   */
  get isConnecting(): boolean {
    return this._state === 'connecting' || this._state === 'reconnecting';
  }

  /**
   * Register a reconnect handler callback.
   */
  setReconnectHandler(handler?: () => Promise<void> | void): void {
    this._reconnectHandler = handler;
  }

  /**
   * Trigger reconnection attempt.
   * Transitions to 'reconnecting', invokes the registered handler (if any),
   * and transitions to 'connected' on success or 'disconnected' on error.
   */
  async reconnect(): Promise<void> {
    this.setState('reconnecting');
    if (this._reconnectHandler) {
      try {
        await this._reconnectHandler();
        this.setState('connected');
      } catch (err) {
        this.setState('disconnected');
        throw err;
      }
    } else {
      // Default reconnect behavior if no handler registered
      this.setState('connected');
    }
  }

  /**
   * Svelte store contract `subscribe` method.
   * Immediately calls listener with current state, then notifies on every state change.
   */
  subscribe(fn: (state: ConnectionState) => void): () => void {
    fn(this._state);
    this._listeners.add(fn);
    return () => {
      this._listeners.delete(fn);
    };
  }

  private notify(): void {
    for (const listener of this._listeners) {
      try {
        listener(this._state);
      } catch (e) {
        console.error('Error in ConnectionManager listener:', e);
      }
    }
  }

  /**
   * Start listening to browser window online/offline events.
   */
  enableBrowserStatusDetection(): void {
    if (typeof window === 'undefined') return;
    this.disableBrowserStatusDetection();

    const handleOffline = () => {
      this.setState('disconnected');
    };

    const handleOnline = () => {
      void this.reconnect().catch(() => {});
    };

    window.addEventListener('offline', handleOffline);
    window.addEventListener('online', handleOnline);

    if (typeof navigator !== 'undefined' && !navigator.onLine) {
      this.setState('disconnected');
    }

    this._cleanupBrowserListeners = () => {
      window.removeEventListener('offline', handleOffline);
      window.removeEventListener('online', handleOnline);
    };
  }

  /**
   * Stop listening to browser window online/offline events.
   */
  disableBrowserStatusDetection(): void {
    if (this._cleanupBrowserListeners) {
      this._cleanupBrowserListeners();
      this._cleanupBrowserListeners = undefined;
    }
  }

  /**
   * Clean up all listeners and browser hooks.
   */
  destroy(): void {
    this.disableBrowserStatusDetection();
    this._listeners.clear();
  }
}

/**
 * Singleton instance of ConnectionManager.
 */
export const connectionManager = new ConnectionManager({
  autoDetectBrowserStatus: typeof window !== 'undefined',
});

/**
 * Helper factory function.
 */
export function createConnectionManager(options?: ConnectionManagerOptions): ConnectionManager {
  return new ConnectionManager(options);
}
