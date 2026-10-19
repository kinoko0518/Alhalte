/**
 * Test double for `RoomSocket` used by the App-level send-path tests.
 *
 * It reproduces only the surface `App.svelte` depends on:
 *
 * - `isConnected` — toggled by the test to simulate going offline/online.
 * - `setChunkLogManager` / `subscribe` — the wiring `App` performs on mount.
 * - `requestAppendLog` — returns a Promise that stays pending until the test
 *   delivers a `LogEventAppended` echo via {@link FakeRoomSocket.deliverEcho},
 *   which appends the record to the chunk manager *before* notifying
 *   subscribers, exactly like `RoomSocket.handleServerFrame`.
 * - `appendLog` — recorded so tests can prove the offline path never uses the
 *   in-memory `messageQueue` (that would double-send on reconnect, Issue #109).
 * - `queryLogBiblio` / `queryLogChunk` — the eager latest-chunk load path
 *   (Issue #109 / sub-issue #140). By default they answer with an empty biblio
 *   so unrelated tests settle; set {@link FakeRoomSocket.autoAnswerLogQueries}
 *   to `false` to drive them manually via {@link FakeRoomSocket.resolveLogBiblio}
 *   / {@link FakeRoomSocket.resolveLogChunk}.
 * - `queryState` / `requestUpdateState` / `requestDeleteState` — the room asset
 *   metadata path (Issue #109 / sub-issue #141), the BGM state path
 *   (Issue #109 / sub-issue #142) and the extension state path
 *   (Issue #109 / sub-issue #145). Queries answer with
 *   {@link FakeRoomSocket.stateSnapshot} unless
 *   {@link FakeRoomSocket.autoAnswerStateQueries} is off, in which case the test
 *   settles them through {@link FakeRoomSocket.resolveStateQuery} /
 *   {@link FakeRoomSocket.rejectStateQuery}; writes resolve immediately unless
 *   {@link FakeRoomSocket.autoAnswerStateWrites} is off, in which case the test
 *   settles them through {@link FakeRoomSocket.resolveStateWrite} /
 *   {@link FakeRoomSocket.rejectStateWrite} /
 *   {@link FakeRoomSocket.rejectDeleteState}.
 * - `requestCreateScene` / `requestDeleteScene` / `requestRewindLog` /
 *   `requestMoveCharacter` — the scene management actions
 *   (Issue #109 / sub-issue #143). They resolve immediately unless
 *   {@link FakeRoomSocket.autoAnswerSceneActions} is off, in which case the test
 *   settles them through {@link FakeRoomSocket.resolveSceneAction} /
 *   {@link FakeRoomSocket.rejectSceneAction}.
 * - `queryListCharacters` / `requestCreateCharacter` / `requestUpdateCharacter`
 *   / `requestDeleteCharacter` — the character tab and editor
 *   (Issue #109 / sub-issue #144). The query answers with
 *   {@link FakeRoomSocket.characterListSnapshot} unless
 *   {@link FakeRoomSocket.autoAnswerCharacterQueries} is off; the writes answer
 *   immediately unless {@link FakeRoomSocket.autoAnswerCharacterRequests} is
 *   off, in which case the test settles them with
 *   {@link FakeRoomSocket.resolveCharacterRequest} /
 *   {@link FakeRoomSocket.rejectCharacterRequest}. Creates allocate ids from
 *   {@link FakeRoomSocket.nextCharacterId}, mirroring `CharacterCreated`.
 * - `requestQueryPermissions` / `requestSetMemberPermission` /
 *   `requestSetDefaultPermission` — the room permission table (Issue #201).
 *   They answer immediately with {@link FakeRoomSocket.permissionsSnapshot}
 *   (mutated by the two writes) unless
 *   {@link FakeRoomSocket.permissionsError} is set, in which case they reject
 *   like a `SetMemberPermissionFailed` / timeout. `emitFrame` can push a
 *   `PermissionsUpdated` to subscribers to simulate a pushed role change.
 * - `emitFrame` — pushes an arbitrary `ServerFrame` to the subscribers so an
 *   App-level test can drive `StateUpdated` / `StateDeleted`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`, `### アセット`
 * - GitHub Issue #109 / sub-issue #139, #140, #141, #142, #143, #144, #145
 * - GitHub Issue #201: 4 段階権限を WebSocket で変更する
 */
import type { ChunkLogManager } from '../../src/utils/log.js';
import type { PermissionsAck, RoomSocket } from '../../src/utils/roomSocket.js';
import type { RoomPermissionLevel } from '../../src/utils/session.js';
import type { Character, LogRecord } from '../../src/types/index.js';

type FrameListener = (frame: unknown) => void;

interface AppendCall {
  sceneId: string;
  record: LogRecord;
}

interface BiblioCall {
  sceneId: string;
}

interface ChunkCall {
  sceneId: string;
  chunkIndex: number;
}

/** One recorded `requestUpdateState` call. */
export interface StateWriteCall {
  sceneId: string;
  key: string;
  value: unknown;
}

/** One recorded `requestDeleteState` call. */
export interface StateDeleteCall {
  sceneId: string;
  key: string;
}

/** One recorded `requestCreateScene` call (Issue #109 / #143, #159). */
export interface CreateSceneCall {
  sceneId: string;
  inheritJuices: boolean;
  /** Optional `std::name` written atomically with the creation. */
  name?: string;
}

/** One recorded `requestDeleteScene` call (Issue #109 / #143). */
export interface DeleteSceneCall {
  sceneId: string;
}

/** One recorded `requestRewindLog` call (Issue #109 / #143). */
export interface RewindLogCall {
  sceneId: string;
  chunkIndex: number;
  position: number;
}

/** One recorded `requestMoveCharacter` call (Issue #109 / #143). */
export interface MoveCharacterCall {
  characterId: number;
  toSceneId: string;
}

/** Kind of a scene action held back by {@link FakeRoomSocket.autoAnswerSceneActions}. */
export type SceneActionKind = 'create' | 'delete' | 'rewind' | 'move';

/** Kind of a character request held back by {@link FakeRoomSocket.autoAnswerCharacterRequests}. */
export type CharacterRequestKind = 'create' | 'update' | 'delete';

/** One recorded `requestCreateCharacter` call (Issue #109 / #144). */
export interface CreateCharacterCall {
  character: Character;
  /** Id the matching `CharacterCreated` ack will carry. */
  allocatedId: number;
}

/** One recorded `requestUpdateCharacter` call (Issue #109 / #144). */
export interface UpdateCharacterCall {
  characterId: number;
  character: Character;
}

/** One recorded `requestDeleteCharacter` call (Issue #109 / #144). */
export interface DeleteCharacterCall {
  characterId: number;
}

interface PendingCharacterRequest {
  kind: CharacterRequestKind;
  /** Server-allocated id used by a successful create ack. */
  allocatedId: number;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

interface PendingSceneAction {
  kind: SceneActionKind;
  label: string;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

interface PendingStateOperation {
  kind: 'update' | 'delete';
  sceneId: string;
  key: string;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

interface PendingLogQuery {
  kind: 'biblio' | 'chunk';
  sceneId: string;
  chunkIndex: number;
  resolve: (value: any) => void;
  reject: (error: unknown) => void;
}

export class FakeRoomSocket {
  /** Whether `App` should treat the socket as connected. */
  isConnected = true;
  /** Chunk manager handed over by `App` on mount. */
  chunkManager: ChunkLogManager | null = null;
  /**
   * When `true` (the default) log queries resolve immediately: `queryLogBiblio`
   * with an empty biblio (no head, so no chunk request) and `queryLogChunk`
   * with no entries. Tests that exercise the loading state set this to `false`
   * and answer the recorded calls themselves.
   */
  autoAnswerLogQueries = true;
  /** Scene state answered by `queryState` (already Tin-decoded values). */
  stateSnapshot: Record<string, unknown> = {};
  /**
   * When `true`, `requestUpdateState` / `requestDeleteState` also mutate
   * {@link FakeRoomSocket.stateSnapshot} like the server would. A test can then
   * unmount / remount the App and observe that the persisted room state was the
   * authority (Issue #165 reload / reconnect checks). Defaults to `false` so
   * existing tests keep a fixed snapshot.
   */
  applyStateWritesToSnapshot = false;
  /**
   * When `true` (the default) `queryState` resolves immediately with
   * {@link FakeRoomSocket.stateSnapshot}. Set to `false` to hold the tab-open
   * query and settle it through {@link FakeRoomSocket.resolveStateQuery} /
   * {@link FakeRoomSocket.rejectStateQuery} (Issue #109 / #145).
   */
  autoAnswerStateQueries = true;
  /**
   * When `true` (the default) state writes / deletes resolve immediately.
   * Set to `false` to drive them with {@link FakeRoomSocket.resolveStateWrite}
   * / {@link FakeRoomSocket.rejectStateWrite} /
   * {@link FakeRoomSocket.rejectDeleteState}.
   */
  autoAnswerStateWrites = true;
  /**
   * When `true` (the default) scene management requests resolve immediately.
   * Set to `false` to drive them with {@link FakeRoomSocket.resolveSceneAction}
   * / {@link FakeRoomSocket.rejectSceneAction} (Issue #109 / #143).
   */
  autoAnswerSceneActions = true;
  /** List answered by `queryListCharacters` (Issue #109 / #144). */
  characterListSnapshot: Character[] = [];
  /**
   * When `true` (the default) the tab-open `ListCharacters` query resolves
   * immediately with {@link FakeRoomSocket.characterListSnapshot}. Set to
   * `false` to hold it and answer through
   * {@link FakeRoomSocket.resolveCharacterListQuery} /
   * {@link FakeRoomSocket.rejectCharacterListQuery}.
   */
  autoAnswerCharacterQueries = true;
  /**
   * When `true` (the default) character create/update/delete requests resolve
   * immediately (creates allocate ids from
   * {@link FakeRoomSocket.nextCharacterId}). Set to `false` to settle them with
   * {@link FakeRoomSocket.resolveCharacterRequest} /
   * {@link FakeRoomSocket.rejectCharacterRequest}.
   */
  autoAnswerCharacterRequests = true;
  /** Id allocated by the next successful `requestCreateCharacter` ack. */
  nextCharacterId = 1000;
  /**
   * Wire permission table answered by `requestQueryPermissions` (Issue #201).
   * The two set requests mutate it and answer with the new table, exactly like
   * banquette's `PermissionsSnapshot` / `PermissionsUpdated`.
   */
  permissionsSnapshot: PermissionsAck = {
    default_permission: 'Viewer',
    permissions: {},
  };
  /**
   * When set, every permission request rejects with this error (simulates a
   * non-admin `SetMemberPermissionFailed` / `SetDefaultPermissionFailed` or a
   * timeout, Issue #201).
   */
  permissionsError: unknown = null;

  readonly requestAppendLogCalls: AppendCall[] = [];
  readonly appendLogCalls: AppendCall[] = [];
  readonly updateStates: Array<{ sceneId: string; key: string; value: unknown }> = [];
  readonly queryLogBiblioCalls: BiblioCall[] = [];
  readonly queryLogChunkCalls: ChunkCall[] = [];
  readonly queryStateCalls: Array<{ sceneId: string }> = [];
  readonly requestUpdateStateCalls: StateWriteCall[] = [];
  readonly requestDeleteStateCalls: StateDeleteCall[] = [];
  readonly requestCreateSceneCalls: CreateSceneCall[] = [];
  readonly requestDeleteSceneCalls: DeleteSceneCall[] = [];
  readonly requestRewindLogCalls: RewindLogCall[] = [];
  readonly requestMoveCharacterCalls: MoveCharacterCall[] = [];
  readonly queryListCharactersCalls: unknown[] = [];
  readonly requestCreateCharacterCalls: CreateCharacterCall[] = [];
  readonly requestUpdateCharacterCalls: UpdateCharacterCall[] = [];
  readonly requestDeleteCharacterCalls: DeleteCharacterCall[] = [];
  /** Count of `QueryPermissions` requests (Issue #201). */
  requestQueryPermissionsCalls = 0;
  /** Recorded `SetMemberPermission` frames (Issue #201). */
  readonly requestSetMemberPermissionCalls: Array<{
    accountId: number;
    permission: RoomPermissionLevel;
  }> = [];
  /** Recorded `SetDefaultPermission` frames (Issue #201). */
  readonly requestSetDefaultPermissionCalls: RoomPermissionLevel[] = [];

  private readonly listeners = new Set<FrameListener>();
  private readonly pending: Array<{
    sceneId: string;
    record: LogRecord;
    resolve: (ack: unknown) => void;
    reject: (error: unknown) => void;
  }> = [];
  private readonly pendingLogQueries: PendingLogQuery[] = [];
  private readonly pendingStateOperations: PendingStateOperation[] = [];
  private readonly pendingStateQueries: Array<{
    sceneId: string;
    resolve: (value: Record<string, unknown>) => void;
    reject: (error: unknown) => void;
  }> = [];
  private readonly pendingSceneActions: PendingSceneAction[] = [];
  private readonly pendingListCharacterQueries: Array<{
    resolve: (value: Character[]) => void;
    reject: (error: unknown) => void;
  }> = [];
  private readonly pendingCharacterRequests: PendingCharacterRequest[] = [];

  setChunkLogManager(manager?: ChunkLogManager): void {
    this.chunkManager = manager ?? null;
  }

  subscribe(listener: FrameListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  requestAppendLog(sceneId: string, record: LogRecord): Promise<unknown> {
    this.requestAppendLogCalls.push({ sceneId, record });
    return new Promise((resolve, reject) => {
      this.pending.push({ sceneId, record, resolve, reject });
    });
  }

  /** Fire-and-forget path: `App` must NOT use this while offline (Issue #109). */
  appendLog(sceneId: string, record: LogRecord): void {
    this.appendLogCalls.push({ sceneId, record });
  }

  updateState(sceneId: string, key: string, value: unknown): void {
    this.updateStates.push({ sceneId, key, value });
  }

  deleteState(): void {}

  /**
   * Answers a scene state query with {@link FakeRoomSocket.stateSnapshot}, or
   * holds it when {@link FakeRoomSocket.autoAnswerStateQueries} is off.
   */
  queryState(sceneId: string): Promise<Record<string, unknown>> {
    this.queryStateCalls.push({ sceneId });
    if (this.autoAnswerStateQueries) {
      return Promise.resolve({ ...this.stateSnapshot });
    }
    return new Promise((resolve, reject) => {
      this.pendingStateQueries.push({ sceneId, resolve, reject });
    });
  }

  /** Number of scene state queries held for a manual answer. */
  get pendingStateQueryCount(): number {
    return this.pendingStateQueries.length;
  }

  /**
   * Resolves the oldest held `queryState` call (optionally one for `sceneId`)
   * with the given decoded state map.
   */
  resolveStateQuery(states: Record<string, unknown>, sceneId?: string): void {
    this.takeStateQuery(sceneId).resolve({ ...states });
  }

  /**
   * Rejects the oldest held `queryState` call (e.g. with an
   * `OperationTimeoutError`), simulating the offline state.
   */
  rejectStateQuery(error: unknown, sceneId?: string): void {
    this.takeStateQuery(sceneId).reject(error);
  }

  private takeStateQuery(sceneId?: string): {
    sceneId: string;
    resolve: (value: Record<string, unknown>) => void;
    reject: (error: unknown) => void;
  } {
    const index = this.pendingStateQueries.findIndex(
      (query) => sceneId === undefined || query.sceneId === sceneId,
    );
    if (index < 0) {
      throw new Error(
        `FakeRoomSocket: no pending state query${sceneId ? ` (${sceneId})` : ''}`,
      );
    }
    return this.pendingStateQueries.splice(index, 1)[0];
  }

  /**
   * Records a room asset metadata write (`asset::<id>`).
   *
   * Resolves with a `StateUpdated`-shaped ack by default so the upload path
   * settles; disable {@link FakeRoomSocket.autoAnswerStateWrites} to settle it
   * manually.
   */
  requestUpdateState(sceneId: string, key: string, value: unknown): Promise<unknown> {
    this.requestUpdateStateCalls.push({ sceneId, key, value });
    if (this.applyStateWritesToSnapshot) {
      this.stateSnapshot[key] = value;
    }
    if (this.autoAnswerStateWrites) {
      return Promise.resolve({ scene_id: sceneId, key, value });
    }
    return new Promise((resolve, reject) => {
      this.pendingStateOperations.push({ kind: 'update', sceneId, key, resolve, reject });
    });
  }

  /**
   * Records a room asset metadata delete (`asset::<id>`).
   */
  requestDeleteState(sceneId: string, key: string): Promise<unknown> {
    this.requestDeleteStateCalls.push({ sceneId, key });
    if (this.applyStateWritesToSnapshot) {
      delete this.stateSnapshot[key];
    }
    if (this.autoAnswerStateWrites) {
      return Promise.resolve({ scene_id: sceneId, key });
    }
    return new Promise((resolve, reject) => {
      this.pendingStateOperations.push({ kind: 'delete', sceneId, key, resolve, reject });
    });
  }

  /** Number of state writes / deletes still waiting for a manual answer. */
  get pendingStateOperationCount(): number {
    return this.pendingStateOperations.length;
  }

  // -------------------------------------------------------------------------
  // Scene management actions (Issue #109 / sub-issue #143)
  // -------------------------------------------------------------------------

  /** Records a `CreateScene` request (resolves with the `SceneCreated` ack shape). */
  requestCreateScene(sceneId: string, inheritJuices = false, name?: string): Promise<unknown> {
    this.requestCreateSceneCalls.push({ sceneId, inheritJuices, ...(name ? { name } : {}) });
    if (this.autoAnswerSceneActions) return Promise.resolve({ scene_id: sceneId });
    return this.holdSceneAction('create', `create:${sceneId}`);
  }

  /** Records a `DeleteScene` request (resolves with the `SceneDeleted` ack shape). */
  requestDeleteScene(sceneId: string, _timeoutMs?: number): Promise<unknown> {
    this.requestDeleteSceneCalls.push({ sceneId });
    if (this.autoAnswerSceneActions) return Promise.resolve({ scene_id: sceneId });
    return this.holdSceneAction('delete', `delete:${sceneId}`);
  }

  /** Records a `RewindLog` request (resolves with the `LogEventRewound` ack shape). */
  requestRewindLog(
    sceneId: string,
    chunkIndex: number,
    position: number,
    _timeoutMs?: number,
  ): Promise<unknown> {
    this.requestRewindLogCalls.push({ sceneId, chunkIndex, position });
    if (this.autoAnswerSceneActions) {
      return Promise.resolve({ scene_id: sceneId, chunk_index: chunkIndex, position });
    }
    return this.holdSceneAction('rewind', `rewind:${sceneId}`);
  }

  /** Records a `MoveCharacter` request (resolves with the `CharacterMoved` ack shape). */
  requestMoveCharacter(
    characterId: number,
    toSceneId: string,
    _timeoutMs?: number,
  ): Promise<unknown> {
    this.requestMoveCharacterCalls.push({ characterId, toSceneId });
    if (this.autoAnswerSceneActions) {
      return Promise.resolve({ character_id: characterId, to_scene_id: toSceneId });
    }
    return this.holdSceneAction('move', `move:${characterId}:${toSceneId}`);
  }

  /** Number of scene actions still waiting for a manual answer. */
  get pendingSceneActionCount(): number {
    return this.pendingSceneActions.length;
  }

  /** Labels of the scene actions still waiting, oldest first. */
  get pendingSceneActionLabels(): string[] {
    return this.pendingSceneActions.map((action) => action.label);
  }

  /**
   * Resolves the oldest pending scene action, optionally only one of `kind`.
   */
  resolveSceneAction(kind?: SceneActionKind): void {
    this.takeSceneAction(kind).resolve({});
  }

  /**
   * Rejects the oldest pending scene action (e.g. with an
   * `OperationTimeoutError`), optionally only one of `kind`.
   */
  rejectSceneAction(error: unknown, kind?: SceneActionKind): void {
    this.takeSceneAction(kind).reject(error);
  }

  private holdSceneAction(kind: SceneActionKind, label: string): Promise<unknown> {
    return new Promise((resolve, reject) => {
      this.pendingSceneActions.push({ kind, label, resolve, reject });
    });
  }

  private takeSceneAction(kind?: SceneActionKind): PendingSceneAction {
    const index = this.pendingSceneActions.findIndex(
      (action) => kind === undefined || action.kind === kind,
    );
    if (index < 0) {
      throw new Error(`FakeRoomSocket: no pending scene action${kind ? ` (${kind})` : ''}`);
    }
    return this.pendingSceneActions.splice(index, 1)[0];
  }

  // -------------------------------------------------------------------------
  // Character tab & editor (Issue #109 / sub-issue #144)
  // -------------------------------------------------------------------------

  /**
   * Records the tab-open `ListCharacters` request and answers with
   * {@link FakeRoomSocket.characterListSnapshot}, or holds it when
   * {@link FakeRoomSocket.autoAnswerCharacterQueries} is off.
   */
  queryListCharacters(_timeoutMs?: number): Promise<Character[]> {
    this.queryListCharactersCalls.push({});
    if (this.autoAnswerCharacterQueries) {
      return Promise.resolve(this.characterListSnapshot.map((char) => ({ ...char })));
    }
    return new Promise((resolve, reject) => {
      this.pendingListCharacterQueries.push({ resolve, reject });
    });
  }

  /** Number of held `ListCharacters` queries. */
  get pendingListCharacterQueryCount(): number {
    return this.pendingListCharacterQueries.length;
  }

  /** Resolves the oldest held `ListCharacters` query. */
  resolveCharacterListQuery(characters: Character[]): void {
    const pending = this.pendingListCharacterQueries.shift();
    if (!pending) throw new Error('FakeRoomSocket: no pending ListCharacters query');
    pending.resolve(characters);
  }

  /** Rejects the oldest held `ListCharacters` query (e.g. with a timeout). */
  rejectCharacterListQuery(error: unknown): void {
    const pending = this.pendingListCharacterQueries.shift();
    if (!pending) throw new Error('FakeRoomSocket: no pending ListCharacters query');
    pending.reject(error);
  }

  /**
   * Records a `CreateCharacter` request and resolves with a `CharacterCreated`
   * ack carrying {@link FakeRoomSocket.nextCharacterId}, or holds it when
   * {@link FakeRoomSocket.autoAnswerCharacterRequests} is off.
   */
  requestCreateCharacter(character: Character, _timeoutMs?: number): Promise<unknown> {
    const allocatedId = this.nextCharacterId++;
    this.requestCreateCharacterCalls.push({ character, allocatedId });
    if (this.autoAnswerCharacterRequests) {
      return Promise.resolve({ character_id: allocatedId });
    }
    return this.holdCharacterRequest('create', allocatedId);
  }

  /** Records an `UpdateCharacter` request and resolves with a `CharacterUpdated` ack. */
  requestUpdateCharacter(
    characterId: number,
    character: Character,
    _timeoutMs?: number,
  ): Promise<unknown> {
    this.requestUpdateCharacterCalls.push({ characterId, character });
    if (this.autoAnswerCharacterRequests) {
      return Promise.resolve({ character_id: characterId });
    }
    return this.holdCharacterRequest('update', characterId);
  }

  /** Records a `DeleteCharacter` request and resolves with a `CharacterDeleted` ack. */
  requestDeleteCharacter(characterId: number, _timeoutMs?: number): Promise<unknown> {
    this.requestDeleteCharacterCalls.push({ characterId });
    if (this.autoAnswerCharacterRequests) {
      return Promise.resolve({ character_id: characterId });
    }
    return this.holdCharacterRequest('delete', characterId);
  }

  /** Number of character create/update/delete requests still waiting for a manual answer. */
  get pendingCharacterRequestCount(): number {
    return this.pendingCharacterRequests.length;
  }

  // -------------------------------------------------------------------------
  // Room permission table (Issue #201)
  // -------------------------------------------------------------------------

  /** Answers `QueryPermissions` with {@link FakeRoomSocket.permissionsSnapshot}. */
  requestQueryPermissions(_timeoutMs?: number): Promise<PermissionsAck> {
    this.requestQueryPermissionsCalls += 1;
    return this.answerPermissions();
  }

  /**
   * Records a `SetMemberPermission` frame and answers with the updated table
   * (the same shape as banquette's `PermissionsUpdated`).
   */
  requestSetMemberPermission(
    accountId: number,
    permission: RoomPermissionLevel,
    _timeoutMs?: number,
  ): Promise<PermissionsAck> {
    this.requestSetMemberPermissionCalls.push({ accountId, permission });
    if (this.permissionsError) return Promise.reject(this.permissionsError);
    this.permissionsSnapshot = {
      ...this.permissionsSnapshot,
      permissions: { ...this.permissionsSnapshot.permissions, [String(accountId)]: permission },
    };
    return this.answerPermissions();
  }

  /** Records a `SetDefaultPermission` frame and answers with the updated table. */
  requestSetDefaultPermission(
    permission: RoomPermissionLevel,
    _timeoutMs?: number,
  ): Promise<PermissionsAck> {
    this.requestSetDefaultPermissionCalls.push(permission);
    if (this.permissionsError) return Promise.reject(this.permissionsError);
    this.permissionsSnapshot = { ...this.permissionsSnapshot, default_permission: permission };
    return this.answerPermissions();
  }

  private answerPermissions(): Promise<PermissionsAck> {
    if (this.permissionsError) return Promise.reject(this.permissionsError);
    return Promise.resolve({
      default_permission: this.permissionsSnapshot.default_permission,
      permissions: { ...this.permissionsSnapshot.permissions },
    });
  }

  /** Pushes a `PermissionsUpdated` frame to every subscriber (pushed role change). */
  deliverPermissionsUpdated(snapshot: PermissionsAck = this.permissionsSnapshot): void {
    this.permissionsSnapshot = {
      default_permission: snapshot.default_permission,
      permissions: { ...snapshot.permissions },
    };
    this.emitFrame({ PermissionsUpdated: snapshot });
  }

  /**
   * Resolves the oldest held character request, optionally only one of `kind`.
   * A create resolves with the id allocated when the request was recorded.
   */
  resolveCharacterRequest(kind?: CharacterRequestKind): void {
    const { request } = this.takeCharacterRequest(kind);
    request.resolve({ character_id: request.allocatedId });
  }

  /** Rejects the oldest held character request (e.g. with a timeout / permission error). */
  rejectCharacterRequest(error: unknown, kind?: CharacterRequestKind): void {
    this.takeCharacterRequest(kind).request.reject(error);
  }

  private holdCharacterRequest(
    kind: CharacterRequestKind,
    allocatedId: number,
  ): Promise<unknown> {
    return new Promise((resolve, reject) => {
      this.pendingCharacterRequests.push({ kind, allocatedId, resolve, reject });
    });
  }

  private takeCharacterRequest(kind?: CharacterRequestKind): {
    request: PendingCharacterRequest;
  } {
    const index = this.pendingCharacterRequests.findIndex(
      (request) => kind === undefined || request.kind === kind,
    );
    if (index < 0) {
      throw new Error(`FakeRoomSocket: no pending character request${kind ? ` (${kind})` : ''}`);
    }
    const [request] = this.pendingCharacterRequests.splice(index, 1);
    return { request };
  }

  /**
   * Resolves the oldest pending state write (or delete) ack.
   */
  resolveStateWrite(): void {
    const operation = this.pendingStateOperations.shift();
    if (!operation) throw new Error('FakeRoomSocket: no pending state operation');
    operation.resolve({ scene_id: operation.sceneId, key: operation.key });
  }

  /**
   * Rejects the oldest pending state write ack (e.g. with an
   * `OperationTimeoutError`).
   */
  rejectStateWrite(error: unknown): void {
    this.takeStateOperation('update').reject(error);
  }

  /** Rejects the oldest pending state delete ack (e.g. `PermissionDenied`). */
  rejectDeleteState(error: unknown): void {
    this.takeStateOperation('delete').reject(error);
  }

  /**
   * Delivers an arbitrary `ServerFrame` to every `subscribe` listener, exactly
   * like `RoomSocket.notifyServerFrame`. Used by App-level state tests.
   */
  emitFrame(frame: unknown): void {
    for (const listener of this.listeners) listener(frame);
  }

  private takeStateOperation(kind: 'update' | 'delete'): PendingStateOperation {
    const operation = this.pendingStateOperations.find((pending) => pending.kind === kind);
    if (!operation) {
      throw new Error(`FakeRoomSocket: no pending state ${kind}`);
    }
    this.pendingStateOperations.splice(this.pendingStateOperations.indexOf(operation), 1);
    return operation;
  }

  enterScene(): void {}

  /**
   * Eager latest-chunk load: records the request and either answers with an
   * empty biblio or waits for {@link FakeRoomSocket.resolveLogBiblio}.
   */
  queryLogBiblio(sceneId: string): Promise<any> {
    this.queryLogBiblioCalls.push({ sceneId });
    if (this.autoAnswerLogQueries) {
      return Promise.resolve({ chunks: [], head: null });
    }
    return new Promise((resolve, reject) => {
      this.pendingLogQueries.push({ kind: 'biblio', sceneId, chunkIndex: -1, resolve, reject });
    });
  }

  /**
   * Eager latest-chunk load: records the request and either answers with an
   * empty chunk or waits for {@link FakeRoomSocket.resolveLogChunk}.
   */
  queryLogChunk(sceneId: string, chunkIndex: number): Promise<any[]> {
    this.queryLogChunkCalls.push({ sceneId, chunkIndex });
    if (this.autoAnswerLogQueries) {
      return Promise.resolve([]);
    }
    return new Promise((resolve, reject) => {
      this.pendingLogQueries.push({ kind: 'chunk', sceneId, chunkIndex, resolve, reject });
    });
  }

  /** Number of biblio/chunk queries still waiting for a manual answer. */
  get pendingLogQueryCount(): number {
    return this.pendingLogQueries.length;
  }

  /**
   * Resolves the oldest pending biblio query (optionally for a specific scene).
   */
  resolveLogBiblio(biblio: unknown, sceneId?: string): void {
    const operation = this.takeLogQuery('biblio', sceneId);
    // Mirror RoomSocket.handleServerFrame: the biblio is authoritative for the
    // scene's cache and registers the chunk list before the chunk arrives.
    this.chunkManager?.applyServerBiblio(operation.sceneId, biblio);
    operation.resolve(biblio);
  }

  /**
   * Resolves the oldest pending chunk query and ingests its entries into the
   * chunk manager, mirroring `RoomSocket.handleServerFrame`: the pending
   * operation settles first, then the `LogChunkSnapshot` replaces the chunk's
   * entries with positional IDs (`log-<chunkIndex>-<position>`) unless one is
   * already present.
   */
  resolveLogChunk(entries: LogRecord[], sceneId?: string, chunkIndex?: number): void {
    const operation = this.takeLogQuery('chunk', sceneId);
    const resolvedChunkIndex = chunkIndex ?? operation.chunkIndex;

    // Same order as the real socket: dispatch the acknowledgement, then ingest
    // the snapshot as the authoritative content of its chunk.
    operation.resolve(entries);
    this.chunkManager?.ingestChunkSnapshot(
      operation.sceneId,
      resolvedChunkIndex,
      entries.map((entry, position) => ({
        ...entry,
        id: entry.id || `log-${resolvedChunkIndex}-${position}`,
        sceneId: entry.sceneId || operation.sceneId,
        chunkIndex: resolvedChunkIndex,
        position,
      })),
    );
  }

  /** Rejects the oldest pending log query (optionally for a specific scene). */
  rejectLogQuery(error: unknown, sceneId?: string): void {
    const operation = this.pendingLogQueries.find(
      (query) => sceneId === undefined || query.sceneId === sceneId,
    );
    if (!operation) {
      throw new Error('FakeRoomSocket: no pending log query');
    }
    this.pendingLogQueries.splice(this.pendingLogQueries.indexOf(operation), 1);
    operation.reject(error);
  }

  private takeLogQuery(kind: 'biblio' | 'chunk', sceneId?: string): PendingLogQuery {
    const operation = this.pendingLogQueries.find(
      (query) => query.kind === kind && (sceneId === undefined || query.sceneId === sceneId),
    );
    if (!operation) {
      throw new Error(`FakeRoomSocket: no pending ${kind} query`);
    }
    this.pendingLogQueries.splice(this.pendingLogQueries.indexOf(operation), 1);
    return operation;
  }

  disconnect(): void {
    this.isConnected = false;
  }

  /** Number of append operations still waiting for an echo. */
  get pendingCount(): number {
    return this.pending.length;
  }

  /** Resolves the oldest pending append by delivering its `LogEventAppended` echo. */
  deliverEcho(position = 0): void {
    const operation = this.pending.shift();
    if (!operation) throw new Error('FakeRoomSocket: no pending append operation');
    this.chunkManager?.appendRecord(operation.record);
    const frame = {
      LogEventAppended: {
        scene_id: operation.sceneId,
        chunk_index: 0,
        position,
        entry: null,
      },
    };
    for (const listener of this.listeners) listener(frame);
    operation.resolve(frame.LogEventAppended);
  }

  /** Rejects the oldest pending append, e.g. with an `OperationTimeoutError`. */
  rejectPending(error: unknown): void {
    const operation = this.pending.shift();
    if (!operation) throw new Error('FakeRoomSocket: no pending append operation');
    operation.reject(error);
  }
}

/** `App` takes a real `RoomSocket`; tests hand it this structural double. */
export function asRoomSocket(socket: FakeRoomSocket): RoomSocket {
  return socket as unknown as RoomSocket;
}
