<script lang="ts">
  /**
   * Extension Management Tab.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 拡張機能`
   * - GitHub Issue #109 / sub-issue #145
   *
   * ## Server synchronization
   *
   * The tab is mounted only while it is the active tab, so mounting means "the
   * user opened the extensions tab". On mount it sends `QueryState('/')` and
   * merges every `extension::<id>` metadata entry into the list
   * (`utils/extensions.ts`); missing wasm bodies are downloaded from the cellar
   * and registered with the pipeline through `onRegisterExtension`.
   *
   * ## One-time seed and server authority (Issue #165)
   *
   * When the room root state has no `extension::__seeded` marker, a GM writes
   * the official defaults the server does not list and then the marker. After
   * that the server state is authoritative: an official default that is not
   * listed was deleted and never comes back on reload / reconnect. Rows whose
   * wasm could not be loaded show `wasm がありません` and cannot be enabled
   * (delete / reorder still work so the admin can clean up).
   *
   * ## Ack-before-model
   *
   * Toggle / delete / reorder / upload never update optimistically:
   *
   * ```
   * idle ──operation──▶ pending(row spinner) ──ack──▶ onUpdateExtensions
   *                                        └──timeout/error──▶ offline (list unchanged)
   * ```
   *
   * A failed operation flips the tab into the offline state. Failures never
   * disable the shell: the list stays readable and scrollable and the offline
   * banner keeps full contrast. Only the mutation controls (toggle / delete /
   * reorder / upload) become `disabled` + `aria-disabled`, even for admins,
   * until the connection returns; the reconnect effect then re-runs the query
   * and restores the actions (Issue #149).
   */
  import { SvelteSet } from 'svelte/reactivity';
  import {
    Button,
    CircularProgress,
    IconButton,
    LinearProgress,
    List,
    ListItem,
    LoadingIndicator,
    Menu,
    MenuItem,
    Switch,
  } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { ExtensionItem } from '../../../types/index.js';
  import {
    canManageExtensions,
    deleteExtension,
    reorderExtensions,
    toggleExtension,
    WasmExtensionInstance,
  } from '../../../wasm/runner.js';
  import {
    applyMenuPosition,
    getMenuPositionStyle,
  } from '../../../utils/menuPosition.js';
  import type { ConnectionState } from '../../../utils/connection.js';
  import type { RoomSocket } from '../../../utils/roomSocket.js';
  import {
    EXTENSION_SEEDED_MARKER_VALUE,
    EXTENSION_SEEDED_STATE_KEY,
    EXTENSION_STATE_SCENE_ID,
    applyRemoteExtensions,
    buildExtensionSeed,
    extensionStateKey,
    extensionToMetadata,
    isExtensionStateKey,
    isExtensionStateSeeded,
    loadExtensionWasm,
    uploadExtensionWasm,
  } from '../../../utils/extensions.js';

  let {
    extensions = [],
    isAdmin = false,
    isViewer = false,
    roomSocket = null,
    roomKey,
    connectionState,
    onUpdateExtensions,
    onRegisterExtension,
  }: {
    extensions?: ExtensionItem[];
    isAdmin?: boolean;
    /** Viewers get the read-only (non-admin) extension UI (Issue #156). */
    isViewer?: boolean;
    /**
     * Socket used to query / persist the room extension state (Issue #109 /
     * sub-issue #145). Without a socket (standalone usage / tests) the tab is
     * treated as offline and stays read-only.
     */
    roomSocket?: RoomSocket | null;
    /** 32-byte room key; defaults to the URL fragment (`#<64 hex>`). */
    roomKey?: Uint8Array | null;
    /** Reactive connection state; used to retry the query after a reconnect. */
    connectionState?: ConnectionState;
    /** Receives the merged list **after** the server acknowledged the change. */
    onUpdateExtensions?: (extensions: ExtensionItem[]) => void;
    /** Registers downloaded / uploaded wasm with the pipeline (Issue #145). */
    onRegisterExtension?: (id: string, wasmBytes: Uint8Array) => void;
  } = $props();

  /** Effective admin UI flag: a viewer never manages extensions (Issue #156). */
  let canAdmin = $derived(isAdmin && !isViewer);

  /**
   * Lifecycle of the tab-open `QueryState('/')` round trip:
   *
   * - `idle`: not queried yet (no socket, or the socket is offline).
   * - `loading`: the query / wasm download is in flight (contained indicator).
   * - `loaded`: the authoritative metadata was merged into the list.
   * - `offline`: the query timed out / failed; the local list is kept and the
   *   tab is dimmed until the connection returns.
   */
  type ExtensionLoadState = 'idle' | 'loading' | 'loaded' | 'offline';

  let loadState = $state<ExtensionLoadState>('idle');
  /** True when a write / upload failed and the tab must stay inert. */
  let writeFailed = $state(false);
  /** Socket the current state belongs to; a new socket triggers a re-query. */
  let queriedSocket: RoomSocket | null = null;
  /** Rows whose state write / delete is waiting for the acknowledgement. */
  let pendingIds = $state(new SvelteSet<string>());
  /** Ids already handed to `onRegisterExtension` (avoids re-instantiating). */
  const registeredIds = new Set<string>();

  /** Upload pipeline state for the progress notice. */
  type UploadPhase = 'idle' | 'uploading' | 'persisting';
  let uploadPhase = $state<UploadPhase>('idle');
  let uploadProgress = $state<number | undefined>(undefined);
  let uploadFileName = $state('');
  let uploadError = $state<string | null>(null);
  let extensionFileInput = $state<HTMLInputElement | null>(null);

  let activeMenuExtId = $state<string | null>(null);
  let moreButtonElements = $state<Record<string, HTMLElement | undefined>>({});
  let menuElements = $state<Record<string, HTMLDivElement | undefined>>({});

  /**
   * Connected flag used by the offline guard.
   *
   * `App` passes the reactive `connectionState`; standalone usages fall back to
   * the socket getter. A missing socket counts as offline (there is no server to
   * acknowledge anything).
   */
  let socketConnected = $derived(
    !roomSocket
      ? false
      : connectionState === undefined
        ? roomSocket.isConnected
        : connectionState === 'connected',
  );

  /**
   * Offline: the socket is gone / disconnected, the tab-open query failed, or a
   * write timed out. While true the mutation controls are disabled even for
   * admins, but the list and the banner stay readable (Issue #149).
   */
  let offline = $derived(!socketConnected || loadState === 'offline' || writeFailed);

  let isLoading = $derived(loadState === 'loading');
  let isUploading = $derived(uploadPhase !== 'idle');
  let isBusy = $derived(pendingIds.size > 0 || isUploading);

  /**
   * Guards **only the mutation controls**, never the shell (Issue #149): the
   * list and the offline banner stay readable and scrollable while a viewer
   * cannot manage extensions, the tab is offline, or an operation is pending.
   */
  let mutationsDisabled = $derived(offline || !canAdmin || isBusy);

  let sortedExtensions = $derived.by(() => {
    return [...extensions].sort((a, b) => a.priority - b.priority);
  });

  /**
   * Re-queries the server when the tab opens, when the socket changes, and when
   * the connection returns after an offline transition.
   *
   * `lastConnected` records the previous connectivity so a `disconnected ->
   * connected` edge triggers a retry while a steady `connected` value (e.g. a
   * write timeout) does not loop.
   */
  let lastConnected: boolean | null = null;
  $effect(() => {
    const socket = roomSocket;
    const connected = socketConnected;
    const wasConnected = lastConnected;
    lastConnected = connected;

    if (!socket || !connected) return;

    const shouldQuery =
      queriedSocket !== socket ||
      (wasConnected === false && (loadState === 'offline' || writeFailed));
    if (!shouldQuery) return;

    queriedSocket = socket;
    void queryExtensions(socket);
  });

  $effect(() => {
    if (activeMenuExtId && menuElements[activeMenuExtId] && moreButtonElements[activeMenuExtId]) {
      return applyMenuPosition(
        menuElements[activeMenuExtId],
        moreButtonElements[activeMenuExtId],
        {
          placement: 'bottom-end',
          anchorName: `--ext-menu-anchor-${activeMenuExtId}`,
        }
      );
    }
  });

  /**
   * Queries `/` state, seeds the official defaults exactly once (Issue #165),
   * merges the metadata and downloads missing wasm.
   *
   * Seeding rules:
   * - marker absent + GM: write every official default the server does not list,
   *   then the marker (`extension::__seeded`) **last**. A failed seed therefore
   *   retries on the next connection instead of marking a half-seeded room.
   * - marker absent + non-GM: nothing is written; the official defaults stay in
   *   the read-only local list until a GM opens the tab.
   * - marker present: the server state is authoritative. An official default id
   *   that is not listed was deleted and is not re-added on reload / reconnect.
   */
  async function queryExtensions(socket: RoomSocket): Promise<void> {
    loadState = 'loading';
    writeFailed = false;
    try {
      const states = (await socket.queryState(EXTENSION_STATE_SCENE_ID)) ?? {};
      const remoteEntries = new Map<string, unknown>();
      for (const [key, value] of Object.entries(states)) {
        if (isExtensionStateKey(key)) remoteEntries.set(key, value);
      }

      const seeded = isExtensionStateSeeded(remoteEntries);
      let authoritativeEntries = remoteEntries;
      let keepMissingOfficialDefaults = false;

      if (!seeded) {
        if (canAdmin) {
          const seed = buildExtensionSeed(extensions, remoteEntries);
          const seededEntries = new Map(remoteEntries);
          for (const { key, value } of seed) {
            await socket.requestUpdateState(EXTENSION_STATE_SCENE_ID, key, value);
            seededEntries.set(key, value);
          }
          // The marker write comes last on purpose (see the method docs).
          await socket.requestUpdateState(
            EXTENSION_STATE_SCENE_ID,
            EXTENSION_SEEDED_STATE_KEY,
            EXTENSION_SEEDED_MARKER_VALUE,
          );
          seededEntries.set(EXTENSION_SEEDED_STATE_KEY, EXTENSION_SEEDED_MARKER_VALUE);
          authoritativeEntries = seededEntries;
        } else {
          // Only a GM may seed; a viewer keeps the read-only official set.
          keepMissingOfficialDefaults = true;
        }
      }

      const merged = await applyRemoteExtensions(
        extensions,
        authoritativeEntries,
        loadWasmFromCellar,
        { keepMissingOfficialDefaults },
      );
      registerDownloadedWasm(merged);
      // `App` owns the list: it is replaced only after the merge succeeded.
      onUpdateExtensions?.(merged);
      loadState = 'loaded';
    } catch (error) {
      console.warn('[ExtensionManagementTab] Failed to query the room extension state:', error);
      // Keep the local list and dim the tab (Issue #109 / #145).
      loadState = 'offline';
    }
  }

  /** Download hook used by {@link applyRemoteExtensions}. */
  async function loadWasmFromCellar(
    tinId: string,
    extensionId: string,
  ): Promise<Uint8Array | undefined> {
    if (!roomKey || roomKey.length !== 32) return undefined;
    try {
      return await loadExtensionWasm(tinId, roomKey);
    } catch (error) {
      console.warn(
        `[ExtensionManagementTab] Failed to download the wasm of "${extensionId}":`,
        error,
      );
      return undefined;
    }
  }

  /** Hands downloaded / restored wasm to the pipeline exactly once per id. */
  function registerDownloadedWasm(items: ExtensionItem[]): void {
    if (!onRegisterExtension) return;
    for (const item of items) {
      if (!item.wasmBytes || item.wasmBytes.length === 0) continue;
      if (!item.cellarTinId) continue;
      if (registeredIds.has(item.id)) continue;
      registeredIds.add(item.id);
      onRegisterExtension(item.id, item.wasmBytes);
    }
  }

  /** Mutations are admin-only and impossible while offline or busy. */
  function canMutate(): boolean {
    return canManageExtensions(canAdmin) && !offline && !isBusy;
  }

  /** Items whose persisted fields differ between two lists. */
  function collectChangedItems(
    before: ExtensionItem[],
    after: ExtensionItem[],
  ): ExtensionItem[] {
    const previous = new Map(before.map((item) => [item.id, item]));
    return after.filter((item) => {
      const prev = previous.get(item.id);
      if (!prev) return true;
      return (
        prev.name !== item.name ||
        prev.description !== item.description ||
        prev.iconSvg !== item.iconSvg ||
        prev.enabled !== item.enabled ||
        prev.priority !== item.priority ||
        prev.cellarTinId !== item.cellarTinId
      );
    });
  }

  /**
   * Persists the changed rows, then commits `updated` to `App`.
   *
   * Deletes are sent first (a removed row should not be resurrected by a
   * priority rewrite), then one `requestUpdateState` per changed row. Nothing is
   * applied locally unless every acknowledgement arrived.
   */
  async function commitStateChanges(
    changed: ExtensionItem[],
    updated: ExtensionItem[],
    deleteIds: string[] = [],
  ): Promise<boolean> {
    const socket = roomSocket;
    if (!socket || !socket.isConnected) {
      writeFailed = true;
      return false;
    }

    const pending = new Set<string>([
      ...changed.map((item) => item.id),
      ...deleteIds,
    ]);
    for (const id of pending) pendingIds.add(id);

    try {
      for (const id of deleteIds) {
        await socket.requestDeleteState(EXTENSION_STATE_SCENE_ID, extensionStateKey(id));
      }
      for (const item of changed) {
        await socket.requestUpdateState(
          EXTENSION_STATE_SCENE_ID,
          extensionStateKey(item.id),
          extensionToMetadata(item),
        );
      }
      // Ack received: only now does the local model change.
      onUpdateExtensions?.(updated);
      writeFailed = false;
      return true;
    } catch (error) {
      console.warn('[ExtensionManagementTab] Failed to persist the extension state:', error);
      writeFailed = true;
      return false;
    } finally {
      for (const id of pending) pendingIds.delete(id);
    }
  }

  async function handleToggle(id: string): Promise<void> {
    if (!canMutate()) return;
    const before = extensions;
    try {
      const updated = toggleExtension(before, id, canAdmin);
      await commitStateChanges(collectChangedItems(before, updated), updated);
    } catch {
      // Non-admin blocked (defensive; the UI is disabled as well).
    }
  }

  async function handleDelete(id: string): Promise<void> {
    if (!canMutate()) return;
    activeMenuExtId = null;
    const before = extensions;
    try {
      const updated = deleteExtension(before, id, canAdmin);
      // Removing a row renumbers the priorities of the following rows, so they
      // must be persisted too or the server order drifts from the UI.
      await commitStateChanges(collectChangedItems(before, updated), updated, [id]);
    } catch {
      // Non-admin blocked (defensive).
    }
  }

  async function handleMoveUp(index: number): Promise<void> {
    if (!canMutate() || index <= 0) return;
    const before = extensions;
    try {
      const updated = reorderExtensions(before, index, index - 1, canAdmin);
      await commitStateChanges(collectChangedItems(before, updated), updated);
    } catch {
      // Non-admin blocked (defensive).
    }
  }

  async function handleMoveDown(index: number): Promise<void> {
    if (!canMutate() || index >= sortedExtensions.length - 1) return;
    const before = extensions;
    try {
      const updated = reorderExtensions(before, index, index + 1, canAdmin);
      await commitStateChanges(collectChangedItems(before, updated), updated);
    } catch {
      // Non-admin blocked (defensive).
    }
  }

  function handleUploadClick(): void {
    if (!canMutate()) return;
    uploadError = null;
    extensionFileInput?.click();
  }

  /**
   * Uploads one `.wasm`:
   *
   * 1. read the bytes and validate them with `WasmExtensionInstance`
   *    (ABI version + `self_introduction()` metadata),
   * 2. `uploadExtensionWasm` (room-key encrypted tin in the cellar),
   * 3. `requestUpdateState('/', 'extension::<id>', metadata)` — the metadata
   *    carries the tin id, never the bytes (4 KiB state limit),
   * 4. only after the ack add the row to the list and register the instance.
   *
   * A cellar / state failure flips the tab offline; a wasm validation failure
   * only shows an inline error (it is not a connection problem).
   */
  async function handleUploadFileChange(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    if (!canMutate()) {
      input.value = '';
      return;
    }

    uploadError = null;
    uploadFileName = file.name;
    uploadPhase = 'uploading';
    uploadProgress = 0;

    let bytes: Uint8Array;
    let intro: { name: string; iconSvg: string; description: string } | null = null;
    try {
      bytes = new Uint8Array(await file.arrayBuffer());
      const instance = await WasmExtensionInstance.instantiate(bytes);
      intro = instance.selfIntroduction();
    } catch (error) {
      console.warn('[ExtensionManagementTab] Invalid extension wasm:', error);
      uploadError = '拡張機能の wasm を読み込めませんでした（ABI を確認してください）';
      uploadPhase = 'idle';
      uploadProgress = undefined;
      input.value = '';
      return;
    }

    if (!roomKey || roomKey.length !== 32) {
      uploadError = '部屋鍵がないため、拡張機能を cellar に保存できません';
      uploadPhase = 'idle';
      uploadProgress = undefined;
      input.value = '';
      return;
    }

    const socket = roomSocket;
    if (!socket || !socket.isConnected) {
      writeFailed = true;
      uploadPhase = 'idle';
      uploadProgress = undefined;
      input.value = '';
      return;
    }

    try {
      const { tinId } = await uploadExtensionWasm(bytes, roomKey, {
        onProgress: (progress) => {
          uploadProgress = progress;
        },
      });

      const id = `ext-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      const priority = extensions.reduce((max, ext) => Math.max(max, ext.priority), -1) + 1;
      const newExt: ExtensionItem = {
        id,
        name: intro?.name || file.name.replace(/\.wasm$/i, ''),
        iconSvg: intro?.iconSvg,
        description: intro?.description,
        enabled: true,
        priority,
        wasmBytes: bytes,
        cellarTinId: tinId,
      };

      uploadPhase = 'persisting';
      await socket.requestUpdateState(
        EXTENSION_STATE_SCENE_ID,
        extensionStateKey(id),
        extensionToMetadata(newExt),
      );

      const updated = [...extensions, newExt].sort((a, b) => a.priority - b.priority);
      registeredIds.add(id);
      onRegisterExtension?.(id, bytes);
      onUpdateExtensions?.(updated);
      uploadPhase = 'idle';
    } catch (error) {
      console.warn('[ExtensionManagementTab] Failed to upload the extension wasm:', error);
      uploadError = 'アップロードに失敗しました';
      writeFailed = true;
      uploadPhase = 'idle';
    } finally {
      uploadProgress = undefined;
      input.value = '';
    }
  }
</script>

<div
  class="extension-tab-shell"
  class:extensions-offline={offline}
  class:non-admin-locked={!canAdmin}
  data-testid="extension-management-tab"
>
  {#if offline}
    <!-- Offline: communication with the server failed. The banner stays at full
         contrast and the list below stays readable / scrollable; only the
         mutation controls are disabled (Issue #149). -->
    <div
      class="extension-offline-notice"
      data-testid="extension-offline-notice"
      role="alert"
      aria-live="assertive"
    >
      <Icon>cloud_off</Icon>
      <div class="offline-text">
        <span class="offline-title">サーバーと通信できませんでした</span>
        <span class="offline-hint">
          接続が戻ると自動的に再試行します。変更はサーバーに反映されていません。
        </span>
      </div>
    </div>
  {/if}

  <div class="extension-tab-container">
    <div class="tab-header">
      <div class="title-with-lock">
        <h3 class="tab-title">拡張機能一覧</h3>
        {#if !canAdmin}
          <span class="lock-indicator" data-testid="lock-indicator">（管理者のみ操作可能）</span>
        {/if}
      </div>
      {#if canAdmin}
        <Button
          variant="filled"
          disabled={offline || isBusy}
          aria-disabled={offline || isBusy ? 'true' : undefined}
          onclick={handleUploadClick}
          data-testid="upload-extension-btn"
        >
          拡張機能を追加 (.wasm)
        </Button>
      {/if}
    </div>

    <!-- Hidden file input owned by the tab: the upload is validated, stored in
         the cellar and acknowledged by the server before the row appears. -->
    <input
      type="file"
      accept=".wasm,application/wasm"
      bind:this={extensionFileInput}
      onchange={handleUploadFileChange}
      class="extension-file-input"
      data-testid="extension-file-input"
    />

    {#if isUploading}
      <div
        class="extension-upload-notice"
        data-testid="extension-upload-progress"
        role="status"
        aria-live="polite"
        aria-label="拡張機能をアップロード中"
      >
        <span class="upload-filename">{uploadFileName}</span>
        {#if uploadPhase === 'uploading'}
          <LinearProgress
            wavy
            track={false}
            indeterminate={uploadProgress === undefined}
            value={uploadProgress ?? 0}
            max={1}
            aria-label="wasm を cellar にアップロード中"
            data-testid="extension-upload-progress-bar"
          />
        {:else}
          <div class="upload-persisting">
            <CircularProgress
              indeterminate
              wavy
              aria-label="サーバーに登録中"
              data-testid="extension-upload-pending"
            />
            <span>サーバーに登録中…</span>
          </div>
        {/if}
      </div>
    {/if}

    {#if uploadError}
      <div class="extension-upload-error" data-testid="extension-upload-error" role="alert">
        {uploadError}
      </div>
    {/if}

    <!-- Reorderable list:
      ［
        （有効化Switch 拡張機能名 右寄せ削除メニュー）
        ...
      ］※並べ替え可能リスト
    -->
    <div class="extension-list-wrapper">
      {#if sortedExtensions.length === 0}
        <div class="empty-notice" data-testid="no-extensions-notice">
          インストールされた拡張機能はありません
        </div>
      {:else}
        <List class="extension-list" data-testid="extension-list" aria-busy={isBusy}>
          {#each sortedExtensions as ext, idx (ext.id)}
            {@const pending = pendingIds.has(ext.id)}
            {@const wasmMissing = ext.wasmMissing === true}
            <ListItem
              type="text"
              class="extension-item-row{wasmMissing ? ' ext-unavailable' : ''}"
              data-testid="extension-item-{ext.id}"
              data-wasm-missing={wasmMissing ? 'true' : undefined}
            >
              {#snippet start()}
                <div class="ext-start-group">
                  {#if canAdmin}
                    <div class="reorder-col">
                      <button
                        type="button"
                        class="reorder-btn"
                        disabled={mutationsDisabled || idx === 0}
                        aria-disabled={mutationsDisabled || idx === 0 ? 'true' : undefined}
                        onclick={() => handleMoveUp(idx)}
                        data-testid="move-up-btn-{ext.id}"
                        title="優先度を上げる"
                      >
                        ▲
                      </button>
                      <button
                        type="button"
                        class="reorder-btn"
                        disabled={mutationsDisabled || idx === sortedExtensions.length - 1}
                        aria-disabled={mutationsDisabled || idx === sortedExtensions.length - 1
                          ? 'true'
                          : undefined}
                        onclick={() => handleMoveDown(idx)}
                        data-testid="move-down-btn-{ext.id}"
                        title="優先度を下げる"
                      >
                        ▼
                      </button>
                    </div>
                  {/if}
                  <div
                    class="ext-icon-themed"
                    data-testid="ext-icon-{ext.id}"
                    aria-hidden="true"
                  >
                    {#if pending}
                      <!-- The row is waiting for the server ack: the icon becomes
                           a wavy indeterminate spinner (Issue #109 / #145). -->
                      <CircularProgress
                        indeterminate
                        wavy
                        data-testid="ext-pending-{ext.id}"
                      />
                    {:else if ext.iconSvg}
                      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                      {@html ext.iconSvg}
                    {:else}
                      <Icon>extension</Icon>
                    {/if}
                  </div>
                </div>
              {/snippet}

              <span class="ext-name" data-testid="ext-name-{ext.id}">
                {ext.name}
              </span>

              {#snippet supportingText()}
                {#if wasmMissing}
                  <!-- Issue #165: a metadata-only row must never look runnable. -->
                  <span
                    class="ext-wasm-missing"
                    data-testid="ext-wasm-missing-{ext.id}"
                    role="status"
                  >
                    wasm がありません
                  </span>
                {/if}
                {#if ext.description}
                  <span class="ext-description" data-testid="ext-description-{ext.id}">
                    {ext.description}
                  </span>
                {/if}
              {/snippet}

              {#snippet end()}
                <div class="ext-end-actions" onclick={(e) => e.stopPropagation()}>
                  <span class="priority-badge">#{ext.priority + 1}</span>
                  <Switch
                    selected={ext.enabled}
                    disabled={mutationsDisabled || wasmMissing}
                    aria-disabled={mutationsDisabled || wasmMissing ? 'true' : undefined}
                    onchange={() => handleToggle(ext.id)}
                    data-testid="ext-toggle-{ext.id}"
                    aria-label="{ext.name}の有効/無効化"
                    aria-checked={ext.enabled ? 'true' : 'false'}
                    title={wasmMissing ? 'wasm がないため有効化できません' : undefined}
                  />
                  <div class="more-menu-wrapper">
                    <IconButton
                      variant="text"
                      disabled={mutationsDisabled}
                      aria-disabled={mutationsDisabled ? 'true' : undefined}
                      onclick={(e) => {
                        e.stopPropagation();
                        activeMenuExtId = activeMenuExtId === ext.id ? null : ext.id;
                      }}
                      bind:element={moreButtonElements[ext.id]}
                      aria-label="その他"
                      title="その他"
                      data-testid="ext-more-btn-{ext.id}"
                    >
                      <Icon>more_vert</Icon>
                    </IconButton>
                    <Menu
                      bind:element={menuElements[ext.id]}
                      open={activeMenuExtId === ext.id}
                      anchor={moreButtonElements[ext.id]}
                      style={getMenuPositionStyle(moreButtonElements[ext.id], {
                        placement: 'bottom-end',
                        anchorName: `--ext-menu-anchor-${ext.id}`,
                      })}
                      data-testid="ext-menu-{ext.id}"
                      ontoggle={(e) => {
                        if (!e.newState || e.newState === 'closed') {
                          if (activeMenuExtId === ext.id) activeMenuExtId = null;
                        }
                      }}
                    >
                      <MenuItem
                        disabled={mutationsDisabled}
                        aria-disabled={mutationsDisabled ? 'true' : undefined}
                        onclick={() => {
                          activeMenuExtId = null;
                          handleDelete(ext.id);
                        }}
                        data-testid="ext-delete-btn-{ext.id}"
                      >
                        {#snippet start()}<Icon>delete</Icon>{/snippet}
                        削除
                      </MenuItem>
                    </Menu>
                  </div>
                </div>
              {/snippet}
            </ListItem>
          {/each}
        </List>
      {/if}

      {#if isLoading}
        <!-- Contained M3 LoadingIndicator over the cached list while
             `QueryState('/')` is in flight (Issue #109 / #145). -->
        <div
          class="extension-loading-overlay"
          data-testid="extension-loading-overlay"
          role="status"
          aria-label="拡張機能を読み込み中"
          aria-live="polite"
        >
          <LoadingIndicator contained data-testid="extension-loading-indicator" />
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .extension-tab-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    box-sizing: border-box;
  }
  /*
   * Issue #149: neither the offline state nor the non-admin lock may dim or
   * freeze the shell. The list and the offline banner must stay readable and
   * scrollable; only the mutation controls are `disabled` + `aria-disabled`.
   * Do not reintroduce `opacity` / `pointer-events` / `filter` here.
   */
  .extension-offline-notice {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 16px;
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    border-radius: var(--np-shape-corner-medium, 12px);
    margin: 16px 16px 0;
    flex: none;
  }
  .offline-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .offline-title {
    font-weight: 700;
    font-size: 0.95rem;
  }
  .offline-hint {
    font-size: 0.8rem;
    /* Issue #149: full contrast; no dimmed text on the error container. */
    color: var(--np-color-on-error-container);
  }
  .extension-tab-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    padding: 16px;
    box-sizing: border-box;
    overflow-y: auto;
    background: var(--np-color-surface);
    color: var(--np-color-on-surface);
    gap: 16px;
  }
  /*
   * Issue #59 / #145 / #149:
   * 管理者（admin）以外のユーザーが開いた場合もタブは読めるままにし、操作だけを無効化する。
   * オフライン時も同じ方針（オフライン通知は通常のコントラストで表示）。
   * 暗転・クリック透過は行わない（再導入しない）。
   */
  .tab-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .title-with-lock {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tab-title {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 700;
  }
  .lock-indicator {
    font-size: 0.8rem;
    color: var(--np-color-error);
  }
  .extension-file-input {
    display: none;
  }
  .extension-upload-notice {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-surface-container-low);
    color: var(--np-color-on-surface);
    flex: none;
  }
  .upload-filename {
    font-size: 0.85rem;
    font-weight: 600;
  }
  .upload-persisting {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
  }
  .extension-upload-error {
    padding: 8px 16px;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    font-size: 0.85rem;
    flex: none;
  }
  .extension-list-wrapper {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .extension-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    margin: 0;
    list-style: none;
    width: 100%;
  }
  :global(.extension-item-row.np-item) {
    width: 100%;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-surface-container-low);
    box-sizing: border-box;
  }
  .ext-start-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .reorder-col {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .reorder-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 0.65rem;
    padding: 2px 4px;
    color: var(--np-color-on-surface-variant);
  }
  .reorder-btn:disabled {
    color: var(--np-color-outline-variant);
    cursor: not-allowed;
  }
  /*
   * Issue #59 要件:
   * 拡張機能アイコン: SVG の色を無視してテーマの色（var(--np-color-primary)）で塗りつぶし、
   * テーマのより暗い色（var(--np-color-primary-container) または --np-color-surface-container-high）の
   * 広めの padding 付き角丸背景（--np-shape-corner-medium）を付与。
   */
  .ext-icon-themed {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    padding: 10px;
    box-sizing: border-box;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-primary-container, var(--np-color-surface-container-high));
    color: var(--np-color-primary);
    flex-shrink: 0;
  }
  .ext-icon-themed :global(svg) {
    width: 100%;
    height: 100%;
    fill: var(--np-color-primary) !important;
    color: var(--np-color-primary) !important;
  }
  .ext-icon-themed :global(svg *) {
    fill: var(--np-color-primary) !important;
  }
  .ext-name {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
  }
  .ext-description {
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Issue #165: unavailable (wasm がありません) rows stay deletable but cannot be
     enabled, so they are tinted with the error container. */
  .ext-wasm-missing {
    display: inline-block;
    font-size: 0.78rem;
    font-weight: 700;
    color: var(--np-color-error);
    margin-bottom: 2px;
  }
  :global(.extension-item-row.ext-unavailable.np-item) {
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
  }
  :global(.extension-item-row.ext-unavailable .ext-icon-themed) {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-error);
  }
  .ext-end-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .priority-badge {
    font-size: 0.75rem;
    background: var(--np-color-surface-container-highest);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--np-color-on-surface-variant);
  }
  .more-menu-wrapper {
    position: relative;
    display: inline-flex;
  }
  .empty-notice {
    padding: 32px;
    text-align: center;
    color: var(--np-color-on-surface-variant);
    font-size: 0.9rem;
  }
  /*
   * Contained M3 LoadingIndicator floating over the cached list while
   * `QueryState('/')` is in flight. The scrim keeps the cached rows readable.
   */
  .extension-loading-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    background-color: color-mix(in srgb, var(--np-color-surface) 72%, transparent);
  }
</style>
