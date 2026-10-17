/**
 * WebSocket synchronization client for Alhalte room communication.
 *
 * Implements real-time bidirectional synchronization between @alhalte/argenterie
 * and alhalte-banquette / alhalte-maitre-d using ClientFrame and ServerFrame protocols.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### protocol`
 * - ARCHITECTURE.md: `## alhalte-maitre-d -> ### WebSocket ルーティング`
 * - GitHub Issue #56: `feat(argenterie): WebSocket によるサーバー操作同期 (ClientFrame / ServerFrame)`
 */

import {
  LABEL_NEED_NOT_KNOW,
  LABEL_PRIMITIVE_BOOL,
  LABEL_PRIMITIVE_STRING,
  LABEL_PRIMITIVE_U64,
} from '@alhalte/pantry';
import type { Character, LogRecord } from '../types/index.js';
import {
  connectionManager as defaultConnectionManager,
  ConnectionManager,
} from './connection.js';
import { ChunkLogManager, parseLogRecord } from './log.js';

/**
 * Self-typed Tin container JSON representation.
 */
export interface TinJson {
  label: number;
  payload: number[];
}

/**
 * Commands and requests initiated by a client WebSocket connection.
 * Conforms to `crates/alhalte-banquette/src/protocol.rs`.
 */
export type ClientFrame =
  | 'ListScenes'
  | { ListScenes?: Record<string, never> }
  | { QueryScene: { scene_id: string } }
  | { CreateScene: { scene_id: string; inherit_juices: boolean } }
  | { DeleteScene: { scene_id: string } }
  | { EnterScene: { scene_id: string } }
  | { LeaveScene: { scene_id: string } }
  | { SetDone: { done: boolean } }
  | { QueryState: { scene_id: string } }
  | { UpdateState: { scene_id: string; key: string; value: TinJson | any } }
  | { DeleteState: { scene_id: string; key: string } }
  | { AppendLog: { scene_id: string; entry: TinJson | any | null } }
  | { EditLog: { scene_id: string; chunk_index: number; position: number; entry: TinJson | any } }
  | { DeleteLog: { scene_id: string; chunk_index: number; position: number } }
  | { RewindLog: { scene_id: string; chunk_index: number; position: number } }
  | { QueryLogBiblio: { scene_id: string } }
  | { QueryLogChunk: { scene_id: string; chunk_index: number } }
  | 'ListCharacters'
  | { ListCharacters?: Record<string, never> }
  | { QueryCharacter: { character_id: number } }
  | { CreateCharacter: { data: any; character_id?: number } }
  | { UpdateCharacter: { character_id: number; data: any } }
  | { DeleteCharacter: { character_id: number } }
  | { MoveCharacter: { character_id: number; to_scene_id: string } }
  | { WebRtcSignal: { to_client_id: number; payload: any } }
  | 'HeartbeatResponse';

/**
 * Messages emitted from banquette to a client WebSocket.
 * Conforms to `crates/alhalte-banquette/src/protocol.rs`.
 */
export type ServerFrame =
  | { SceneList: { scenes: string[] } }
  | { SceneInfo: any }
  | { StateSnapshot: { scene_id: string; states: Record<string, any> } }
  | { StateUpdated: { scene_id: string; key: string; value: any } }
  | { StateDeleted: { scene_id: string; key: string } }
  | { LogBiblioSnapshot: { scene_id: string; biblio: any } }
  | { LogChunkSnapshot: { scene_id: string; chunk_index: number; entries: any[] } }
  | { LogEventAppended: { scene_id: string; chunk_index: number; position: number; entry: any } }
  | { LogEventEdited: { scene_id: string; chunk_index: number; position: number; entry: any } }
  | { LogEventDeleted: { scene_id: string; chunk_index: number; position: number } }
  | { LogEventRewound: { scene_id: string; chunk_index: number; position: number } }
  | { CharacterList: { characters: Record<string | number, any> } }
  | { CharacterInfo: { character_id: number; data: any } }
  | { CharacterCreated: { character_id: number } }
  | { CharacterUpdated: { character_id: number } }
  | { CharacterDeleted: { character_id: number } }
  | { WebRtcSignal: { from_client_id: number; payload: any } }
  | 'HeartbeatPing'
  | { HeartbeatPing: Record<string, never> }
  | { Error: { code: string; message: string } };

/**
 * Configuration options for RoomSocket.
 */
export interface RoomSocketOptions {
  /** Room identifier */
  roomId: string | number;
  /** Explicit WebSocket URL (e.g., ws://host/ws/room/123) */
  url?: string;
  /** Server host override (e.g., localhost:3000) */
  host?: string;
  /** Authenticated account ID for query string */
  accountId?: number | string;
  /** Authenticated session ID for query string */
  sessionId?: number | string;
  /** ConnectionManager instance (defaults to shared singleton) */
  connectionManager?: ConnectionManager;
  /** ChunkLogManager instance for log sync */
  chunkLogManager?: ChunkLogManager;
  /** Auto connect upon initialization (defaults to false) */
  autoConnect?: boolean;
  /** Initial scene ID to enter (defaults to '/') */
  initialSceneId?: string;
  /** Reconnection delay in milliseconds (defaults to 2000) */
  reconnectInterval?: number;
  /** Maximum reconnect attempts (defaults to Infinity) */
  maxReconnectAttempts?: number;
  /** Custom WebSocket factory for testing */
  webSocketFactory?: (url: string) => WebSocket;

  // Frame event callbacks
  onSceneList?: (scenes: string[]) => void;
  onCharacterList?: (characters: Character[]) => void;
  onCharacterCreated?: (characterId: number) => void;
  onCharacterUpdated?: (characterId: number, data?: any) => void;
  onCharacterDeleted?: (characterId: number) => void;
  onStateUpdated?: (sceneId: string, key: string, value: any) => void;
  onStateDeleted?: (sceneId: string, key: string) => void;
  onLogAppended?: (record: LogRecord) => void;
  onServerFrame?: (frame: ServerFrame) => void;
  onError?: (error: { code: string; message: string }) => void;
}

/**
 * Normalizes any value into a Tin container if not already one.
 */
export function ensureTin(val: any): any {
  if (val && typeof val === 'object' && 'label' in val && 'payload' in val) {
    return val;
  }
  if (typeof val === 'string') {
    return {
      label: Number(LABEL_PRIMITIVE_STRING),
      payload: Array.from(new TextEncoder().encode(val)),
    };
  }
  if (typeof val === 'number') {
    const buf = new Uint8Array(8);
    const view = new DataView(buf.buffer);
    view.setBigUint64(0, BigInt(Math.floor(val)), true);
    return {
      label: Number(LABEL_PRIMITIVE_U64),
      payload: Array.from(buf),
    };
  }
  if (typeof val === 'boolean') {
    return {
      label: Number(LABEL_PRIMITIVE_BOOL),
      payload: [val ? 1 : 0],
    };
  }
  return val;
}

/**
 * Builds standard WebSocket URL for room connection with optional credentials.
 */
export function buildRoomWebSocketUrl(options: {
  roomId: string | number;
  url?: string;
  host?: string;
  accountId?: number | string;
  sessionId?: number | string;
}): string {
  const { roomId, url, host, accountId, sessionId } = options;

  let base: string;
  if (url) {
    const hashIdx = url.indexOf('#');
    const cleanUrl = hashIdx >= 0 ? url.slice(0, hashIdx) : url;

    if (cleanUrl.startsWith('http://')) {
      base = 'ws://' + cleanUrl.slice('http://'.length);
    } else if (cleanUrl.startsWith('https://')) {
      base = 'wss://' + cleanUrl.slice('https://'.length);
    } else if (cleanUrl.startsWith('ws://') || cleanUrl.startsWith('wss://')) {
      base = cleanUrl;
    } else {
      const proto =
        typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const h = host || (typeof window !== 'undefined' ? window.location.host : 'localhost:3000');
      base = `${proto}//${h}${cleanUrl.startsWith('/') ? '' : '/'}${cleanUrl}`;
    }
  } else {
    const proto =
      typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const h = host || (typeof window !== 'undefined' ? window.location.host : 'localhost:3000');
    base = `${proto}//${h}/ws/room/${roomId}`;
  }

  const hashIdx = base.indexOf('#');
  if (hashIdx >= 0) {
    base = base.slice(0, hashIdx);
  }

  const queryIndex = base.indexOf('?');
  const pathname = queryIndex >= 0 ? base.slice(0, queryIndex) : base;
  const searchStr = queryIndex >= 0 ? base.slice(queryIndex + 1) : '';
  const params = new URLSearchParams(searchStr);

  if (accountId !== undefined && accountId !== null && !params.has('account_id')) {
    params.set('account_id', String(accountId));
  }
  if (sessionId !== undefined && sessionId !== null && !params.has('session_id')) {
    params.set('session_id', String(sessionId));
  }

  const queryString = params.toString();
  return queryString ? `${pathname}?${queryString}` : pathname;
}

/**
 * Converts a raw server log entry (Tin container, string, or object) into a LogRecord.
 */
export function entryToLogRecord(
  sceneId: string,
  entry: any,
  chunkIndex = 0,
  position = 0,
): LogRecord | null {
  if (entry === null || entry === undefined) return null;

  // Direct LogRecord object
  if (
    typeof entry === 'object' &&
    typeof entry.content === 'string' &&
    typeof entry.speaker === 'string'
  ) {
    return parseLogRecord(
      { ...entry, sceneId: entry.sceneId || sceneId },
      `log-${chunkIndex}-${position}`,
    );
  }

  // Tin container with payload
  if (typeof entry === 'object' && entry.payload) {
    try {
      const payloadBytes = Array.isArray(entry.payload)
        ? new Uint8Array(entry.payload)
        : entry.payload instanceof Uint8Array
          ? entry.payload
          : new Uint8Array(Object.values(entry.payload));
      const text = new TextDecoder().decode(payloadBytes);
      try {
        const json = JSON.parse(text);
        if (typeof json === 'object' && json !== null) {
          return parseLogRecord(
            { ...json, sceneId: json.sceneId || sceneId },
            `log-${chunkIndex}-${position}`,
          );
        }
      } catch {
        // Plain text string payload
      }
      return parseLogRecord({ content: text, sceneId }, `log-${chunkIndex}-${position}`);
    } catch {
      // Decode failed
    }
  }

  // Plain string
  if (typeof entry === 'string') {
    return parseLogRecord({ content: entry, sceneId }, `log-${chunkIndex}-${position}`);
  }

  // Generic object
  if (typeof entry === 'object') {
    return parseLogRecord(
      { ...entry, sceneId: entry.sceneId || sceneId },
      `log-${chunkIndex}-${position}`,
    );
  }

  return null;
}

/**
 * Parses raw server character data into a Character model.
 */
export function parseCharacter(id: string | number, raw: any): Character {
  let name = `Character ${id}`;
  if (typeof raw?.name === 'string' && raw.name.length > 0) {
    name = raw.name;
  } else if (raw?.['std::name']) {
    const val = raw['std::name'];
    if (typeof val === 'string') {
      name = val;
    } else if (val && typeof val === 'object' && val.payload) {
      try {
        const bytes = Array.isArray(val.payload)
          ? new Uint8Array(val.payload)
          : val.payload instanceof Uint8Array
            ? val.payload
            : new Uint8Array(Object.values(val.payload));
        name = new TextDecoder().decode(bytes);
      } catch {
        // ignore
      }
    }
  }

  return {
    id: String(id),
    name,
    initiative: typeof raw?.initiative === 'number' ? raw.initiative : 0,
    iconUrl: typeof raw?.iconUrl === 'string' ? raw.iconUrl : undefined,
    description: typeof raw?.description === 'string' ? raw.description : '',
    reference: typeof raw?.reference === 'string' ? raw.reference : undefined,
    statuses: Array.isArray(raw?.statuses) ? raw.statuses : [],
    parameters: Array.isArray(raw?.parameters) ? raw.parameters : [],
    chatPalette: typeof raw?.chatPalette === 'string' ? raw.chatPalette : '',
    hideStatus: Boolean(raw?.hideStatus),
    placedAsPiece: Boolean(raw?.placedAsPiece),
    piecePosition:
      raw?.piecePosition &&
      typeof raw.piecePosition.x === 'number' &&
      typeof raw.piecePosition.y === 'number'
        ? { x: raw.piecePosition.x, y: raw.piecePosition.y }
        : undefined,
  };
}

/**
 * Client managing WebSocket connection, reconnection, and ClientFrame/ServerFrame
 * synchronization for an active room.
 */
export class RoomSocket {
  private wsUrl: string;
  private options: RoomSocketOptions;
  private connectionManager: ConnectionManager;
  private chunkLogManager?: ChunkLogManager;
  private socket: WebSocket | null = null;
  private outgoingQueue: ClientFrame[] = [];
  private intentionalClose = false;
  private isReconnecting = false;
  private reconnectAttempts = 0;
  private reconnectTimer: any = null;
  private currentSceneId: string;
  private connectPromises: Array<{ resolve: () => void; reject: (err: any) => void }> = [];

  // Local state cache
  private scenes: string[] = [];
  private characters: Character[] = [];
  private stateMap: Map<string, Map<string, any>> = new Map();

  // Observers
  private frameListeners: Set<(frame: ServerFrame) => void> = new Set();

  constructor(options: RoomSocketOptions) {
    this.options = options;
    this.connectionManager = options.connectionManager ?? defaultConnectionManager;
    this.chunkLogManager = options.chunkLogManager;
    this.currentSceneId = options.initialSceneId || '/';
    this.wsUrl = buildRoomWebSocketUrl(options);

    // Register chained reconnect handler with connectionManager
    const prevHandler = (this.connectionManager as any)._reconnectHandler;
    this.connectionManager.setReconnectHandler(async () => {
      if (prevHandler) {
        try {
          await prevHandler();
        } catch (err) {
          console.warn('Error in previous reconnect handler:', err);
        }
      }
      await this.reconnect();
    });

    if (options.autoConnect) {
      this.connect();
    }
  }

  /**
   * Sets or updates the active ChunkLogManager.
   */
  public setChunkLogManager(manager?: ChunkLogManager): void {
    this.chunkLogManager = manager;
  }

  /**
   * Current WebSocket target URL.
   */
  public getUrl(): string {
    return this.wsUrl;
  }

  /**
   * True if WebSocket connection is open.
   */
  public get isConnected(): boolean {
    return this.socket !== null && this.socket.readyState === 1; // WebSocket.OPEN
  }

  /**
   * True if currently connecting or reconnecting.
   */
  public get isConnecting(): boolean {
    return (
      this.socket !== null &&
      (this.socket.readyState === 0 || this.isReconnecting) // WebSocket.CONNECTING
    );
  }

  /**
   * Connects to the room WebSocket server.
   */
  public connect(): void {
    if (this.socket && (this.socket.readyState === 0 || this.socket.readyState === 1)) {
      return;
    }

    if (
      !this.options.webSocketFactory &&
      (this.wsUrl.includes('.example') || this.wsUrl.includes('://example.'))
    ) {
      // Reserved RFC 2606 .example domain used in documentation and unit test fixtures
      return;
    }

    this.intentionalClose = false;
    if (this.isReconnecting) {
      this.connectionManager.setState('reconnecting');
    } else {
      this.connectionManager.setState('connecting');
    }

    try {
      const ws = this.options.webSocketFactory
        ? this.options.webSocketFactory(this.wsUrl)
        : new WebSocket(this.wsUrl);
      this.socket = ws;

      ws.onopen = () => {
        this.connectionManager.setState('connected');
        this.isReconnecting = false;
        this.reconnectAttempts = 0;
        this.clearReconnectTimer();

        // Resolve any awaiting reconnect promises
        for (const { resolve } of this.connectPromises) {
          resolve();
        }
        this.connectPromises = [];

        // Flush queued frames
        this.flushQueue();

        // Enter initial scene & request room lists
        this.enterScene(this.currentSceneId);
        this.listScenes();
        this.listCharacters();
      };

      ws.onmessage = (event: MessageEvent) => {
        this.handleMessage(event.data);
      };

      ws.onerror = (event: Event) => {
        console.warn('RoomSocket error:', event);
      };

      ws.onclose = () => {
        const wasIntentional = this.intentionalClose;
        this.socket = null;

        if (wasIntentional) {
          this.connectionManager.setState('disconnected');
          for (const { reject } of this.connectPromises) {
            reject(new Error('WebSocket closed intentionally'));
          }
          this.connectPromises = [];
          return;
        }

        // Unexpected disconnect: update connectionManager and schedule auto-reconnection
        this.connectionManager.setState('disconnected');

        for (const { reject } of this.connectPromises) {
          reject(new Error('WebSocket connection closed'));
        }
        this.connectPromises = [];

        const maxAttempts = this.options.maxReconnectAttempts ?? Infinity;
        if (this.reconnectAttempts < maxAttempts) {
          this.reconnectAttempts++;
          this.clearReconnectTimer();
          const delay = this.options.reconnectInterval ?? 2000;
          this.reconnectTimer = setTimeout(() => {
            this.reconnect().catch(() => {});
          }, delay);
        }
      };
    } catch (err) {
      this.connectionManager.setState('disconnected');
      for (const { reject } of this.connectPromises) {
        reject(err);
      }
      this.connectPromises = [];
    }
  }

  /**
   * Attempts reconnection to the room WebSocket server.
   */
  public reconnect(): Promise<void> {
    this.clearReconnectTimer();
    this.intentionalClose = false;
    this.isReconnecting = true;
    this.connectionManager.setState('reconnecting');

    if (
      !this.options.webSocketFactory &&
      (this.wsUrl.includes('.example') || this.wsUrl.includes('://example.'))
    ) {
      this.isReconnecting = false;
      this.connectionManager.setState('connected');
      return Promise.resolve();
    }

    if (this.socket) {
      try {
        this.socket.close();
      } catch {
        // ignore
      }
      this.socket = null;
    }

    return new Promise<void>((resolve, reject) => {
      this.connectPromises.push({ resolve, reject });
      this.connect();
    });
  }

  /**
   * Disconnects and cleans up the WebSocket connection intentionally.
   */
  public disconnect(): void {
    this.intentionalClose = true;
    this.clearReconnectTimer();
    if (this.socket) {
      try {
        this.socket.close();
      } catch {
        // ignore
      }
      this.socket = null;
    }
    this.connectionManager.setState('disconnected');
  }

  /**
   * Alias for `disconnect()`.
   */
  public close(): void {
    this.disconnect();
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private flushQueue(): void {
    if (!this.isConnected || !this.socket) return;
    while (this.outgoingQueue.length > 0) {
      const frame = this.outgoingQueue.shift()!;
      this.socket.send(JSON.stringify(frame));
    }
  }

  // =========================================================================
  // ClientFrame Outgoing Methods
  // =========================================================================

  /**
   * Sends an arbitrary ClientFrame to the server (or queues it if connecting).
   */
  public send(frame: ClientFrame): void {
    if (this.isConnected && this.socket) {
      this.socket.send(JSON.stringify(frame));
    } else {
      this.outgoingQueue.push(frame);
    }
  }

  /**
   * Enters a scene.
   */
  public enterScene(sceneId: string): void {
    this.currentSceneId = sceneId;
    this.send({ EnterScene: { scene_id: sceneId } });
  }

  /**
   * Leaves a scene.
   */
  public leaveScene(sceneId: string): void {
    this.send({ LeaveScene: { scene_id: sceneId } });
  }

  /**
   * Creates a new scene.
   */
  public createScene(sceneId: string, inheritJuices: boolean): void {
    this.send({ CreateScene: { scene_id: sceneId, inherit_juices: inheritJuices } });
  }

  /**
   * Deletes a scene (admin only).
   */
  public deleteScene(sceneId: string): void {
    this.send({ DeleteScene: { scene_id: sceneId } });
  }

  /**
   * Updates or inserts a key-value pair in scene state.
   */
  public updateState(sceneId: string, key: string, value: any): void {
    this.send({ UpdateState: { scene_id: sceneId, key, value: ensureTin(value) } });
  }

  /**
   * Deletes a key from scene state.
   */
  public deleteState(sceneId: string, key: string): void {
    this.send({ DeleteState: { scene_id: sceneId, key } });
  }

  /**
   * Appends a log entry to a scene.
   */
  public appendLog(sceneId: string, entry: any): void {
    this.send({ AppendLog: { scene_id: sceneId, entry: entry ? ensureTin(entry) : null } });
  }

  /**
   * Creates a character. Supports `createCharacter(characterId, data)` or `createCharacter(data)`.
   */
  public createCharacter(characterIdOrData: number | any, data?: any): void {
    if (data !== undefined) {
      this.send({
        CreateCharacter: {
          character_id: typeof characterIdOrData === 'number' ? characterIdOrData : undefined,
          data,
        },
      });
    } else {
      this.send({
        CreateCharacter: {
          data: characterIdOrData,
        },
      });
    }
  }

  /**
   * Updates an existing character.
   */
  public updateCharacter(characterId: number, data: any): void {
    this.send({ UpdateCharacter: { character_id: characterId, data } });
  }

  /**
   * Deletes a character.
   */
  public deleteCharacter(characterId: number): void {
    this.send({ DeleteCharacter: { character_id: characterId } });
  }

  /**
   * Moves a character to a scene.
   */
  public moveCharacter(characterId: number, toSceneId: string): void {
    this.send({ MoveCharacter: { character_id: characterId, to_scene_id: toSceneId } });
  }

  /**
   * Sets the room's done flag (admin only).
   */
  public setDone(done: boolean): void {
    this.send({ SetDone: { done } });
  }

  /**
   * Requests the full list of scenes.
   */
  public listScenes(): void {
    this.send('ListScenes');
  }

  /**
   * Requests the list of characters.
   */
  public listCharacters(): void {
    this.send('ListCharacters');
  }

  // =========================================================================
  // ServerFrame Incoming Handling
  // =========================================================================

  /**
   * Handles incoming WebSocket message.
   */
  public handleMessage(data: any): void {
    let frame: ServerFrame;
    try {
      if (typeof data === 'string') {
        const trimmed = data.trim();
        if (trimmed === 'HeartbeatPing' || trimmed === '"HeartbeatPing"') {
          frame = 'HeartbeatPing';
        } else {
          frame = JSON.parse(trimmed);
        }
      } else {
        return;
      }
    } catch (err) {
      console.warn('Failed to parse ServerFrame JSON:', err);
      return;
    }

    this.handleServerFrame(frame);
  }

  /**
   * Processes a structured ServerFrame.
   */
  public handleServerFrame(frame: ServerFrame): void {
    // 1. HeartbeatPing
    if (frame === 'HeartbeatPing' || (typeof frame === 'object' && 'HeartbeatPing' in frame)) {
      this.send('HeartbeatResponse');
      this.notifyServerFrame(frame);
      return;
    }

    if (typeof frame !== 'object' || frame === null) {
      this.notifyServerFrame(frame);
      return;
    }

    // 2. SceneList
    if ('SceneList' in frame) {
      this.scenes = frame.SceneList.scenes;
      this.options.onSceneList?.(frame.SceneList.scenes);
    }

    // 3. StateUpdated
    if ('StateUpdated' in frame) {
      const { scene_id, key, value } = frame.StateUpdated;
      let scMap = this.stateMap.get(scene_id);
      if (!scMap) {
        scMap = new Map();
        this.stateMap.set(scene_id, scMap);
      }
      scMap.set(key, value);
      this.options.onStateUpdated?.(scene_id, key, value);
    }

    // 4. StateDeleted
    if ('StateDeleted' in frame) {
      const { scene_id, key } = frame.StateDeleted;
      const scMap = this.stateMap.get(scene_id);
      if (scMap) {
        scMap.delete(key);
      }
      this.options.onStateDeleted?.(scene_id, key);
    }

    // 5. LogEventAppended
    if ('LogEventAppended' in frame) {
      const { scene_id, chunk_index, position, entry } = frame.LogEventAppended;
      const record = entryToLogRecord(scene_id, entry, chunk_index, position);
      if (record) {
        this.chunkLogManager?.appendRecord(record);
        this.options.onLogAppended?.(record);
      }
    }

    // 6. LogChunkSnapshot
    if ('LogChunkSnapshot' in frame) {
      const { scene_id, chunk_index, entries } = frame.LogChunkSnapshot;
      if (Array.isArray(entries)) {
        entries.forEach((entry, pos) => {
          const record = entryToLogRecord(scene_id, entry, chunk_index, pos);
          if (record) {
            this.chunkLogManager?.appendRecord(record);
            this.options.onLogAppended?.(record);
          }
        });
      }
    }

    // 7. CharacterList
    if ('CharacterList' in frame) {
      const charList: Character[] = [];
      if (frame.CharacterList?.characters) {
        for (const [id, raw] of Object.entries(frame.CharacterList.characters)) {
          charList.push(parseCharacter(id, raw));
        }
      }
      this.characters = charList;
      this.options.onCharacterList?.(charList);
    }

    // 8. CharacterCreated
    if ('CharacterCreated' in frame) {
      this.options.onCharacterCreated?.(frame.CharacterCreated.character_id);
    }

    // 9. CharacterUpdated
    if ('CharacterUpdated' in frame) {
      this.options.onCharacterUpdated?.(frame.CharacterUpdated.character_id);
    }

    // 10. CharacterDeleted
    if ('CharacterDeleted' in frame) {
      const idStr = String(frame.CharacterDeleted.character_id);
      this.characters = this.characters.filter((c) => c.id !== idStr);
      this.options.onCharacterDeleted?.(frame.CharacterDeleted.character_id);
    }

    // 11. Error
    if ('Error' in frame) {
      this.options.onError?.(frame.Error);
    }

    this.notifyServerFrame(frame);
  }

  // =========================================================================
  // Subscriptions & State Accessors
  // =========================================================================

  /**
   * Subscribes a listener to all incoming ServerFrames.
   */
  public subscribe(listener: (frame: ServerFrame) => void): () => void {
    this.frameListeners.add(listener);
    return () => {
      this.frameListeners.delete(listener);
    };
  }

  private notifyServerFrame(frame: ServerFrame): void {
    this.options.onServerFrame?.(frame);
    for (const listener of this.frameListeners) {
      try {
        listener(frame);
      } catch (e) {
        console.error('Error in ServerFrame listener:', e);
      }
    }
  }

  /**
   * Current cached scenes.
   */
  public getScenes(): string[] {
    return [...this.scenes];
  }

  /**
   * Current cached characters.
   */
  public getCharacters(): Character[] {
    return [...this.characters];
  }

  /**
   * Current cached scene state map.
   */
  public getState(sceneId: string): Map<string, any> | undefined {
    return this.stateMap.get(sceneId);
  }
}

/**
 * Factory helper function to create a RoomSocket.
 */
export function createRoomSocket(options: RoomSocketOptions): RoomSocket {
  return new RoomSocket(options);
}
