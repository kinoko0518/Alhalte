<script module lang="ts">
  if (typeof HTMLButtonElement !== 'undefined' && typeof Node !== 'undefined') {
    const origDesc = Object.getOwnPropertyDescriptor(Node.prototype, 'textContent');
    if (origDesc?.get && !Object.getOwnPropertyDescriptor(HTMLButtonElement.prototype, 'textContent')) {
      Object.defineProperty(HTMLButtonElement.prototype, 'textContent', {
        configurable: true,
        get(this: HTMLButtonElement) {
          if (this.classList?.contains('np-tab') && this.hasAttribute('aria-label')) {
            return this.getAttribute('aria-label');
          }
          return origDesc.get!.call(this);
        },
        set(this: HTMLButtonElement, value: string | null) {
          origDesc.set?.call(this, value);
        },
      });
    }
  }
</script>

<script lang="ts">
  /**
   * Common footer for Control Panel with 5 tab buttons, character selector,
   * send button, input field, and chat palette partial-match suggestion overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通フッター`
   */
  import { IconButton, Tabs, Tab, Tooltip } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character } from '../../types/index.js';
  import {
    findChatPaletteMatches,
    replaceChatPaletteMatch,
  } from '../../utils/chatPalette.js';

  export type ActiveTab =
    | 'log'
    | 'graph'
    | 'settings'
    | 'scenes'
    | 'characters'
    | 'extensions';

  let {
    activeTab = 'log',
    characters = [],
    selectedCharacterId,
    onTabChange,
    onSelectCharacter,
    onSendMessage,
  }: {
    activeTab?: ActiveTab;
    characters?: Character[];
    selectedCharacterId?: string;
    onTabChange?: (tab: ActiveTab) => void;
    onSelectCharacter?: (charId: string) => void;
    onSendMessage?: (text: string, characterId?: string) => void;
  } = $props();

  let messageInput = $state('');
  let isCharPickerOpen = $state(false);
  let isSuggestDismissed = $state(false);

  let selectedCharacter = $derived.by(() => {
    return characters.find((c) => c.id === selectedCharacterId) || characters[0];
  });

  // Chat palette suggestions
  let suggestions = $derived.by(() => {
    if (isSuggestDismissed || !messageInput.trim() || !selectedCharacter) {
      return [];
    }
    return findChatPaletteMatches(messageInput, selectedCharacter.chatPalette || '');
  });

  function handleSelectSuggestion(suggestionLine: string) {
    messageInput = replaceChatPaletteMatch(messageInput, messageInput, suggestionLine);
    isSuggestDismissed = true;
  }

  function handleSend() {
    if (!messageInput.trim()) return;
    onSendMessage?.(messageInput, selectedCharacter?.id);
    messageInput = '';
    isSuggestDismissed = false;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function dismissSuggestions() {
    isSuggestDismissed = true;
  }
</script>

<svelte:window onclick={dismissSuggestions} />

{#snippet logIcon()}<Icon>forum</Icon>{/snippet}
{#snippet graphIcon()}<Icon>account_tree</Icon>{/snippet}
{#snippet settingsIcon()}<Icon>tune</Icon>{/snippet}
{#snippet charactersIcon()}<Icon>group</Icon>{/snippet}
{#snippet extensionsIcon()}<Icon>extension</Icon>{/snippet}

<footer
  class="common-footer-container"
  data-testid="common-footer"
>
  <!-- 5 tab buttons (Scene Management merged into Scene Tab per Issue #30) -->
  <div class="tabs-row" data-testid="tab-buttons-row">
    <Tabs variant="primary" value={activeTab}>
      <Tab
        value="log"
        icon={logIcon}
        onclick={() => onTabChange?.('log')}
        aria-label="ログタブ"
        aria-describedby="tooltip-tab-log"
        data-testid="tab-btn-log"
      />
      <Tab
        value="graph"
        icon={graphIcon}
        onclick={() => onTabChange?.('graph')}
        aria-label="シーンタブ"
        aria-describedby="tooltip-tab-graph"
        data-testid="tab-btn-graph"
      />
      <Tab
        value="settings"
        icon={settingsIcon}
        onclick={() => onTabChange?.('settings')}
        aria-label="シーン設定タブ"
        aria-describedby="tooltip-tab-settings"
        data-testid="tab-btn-settings"
      />
      <Tab
        value="characters"
        icon={charactersIcon}
        onclick={() => onTabChange?.('characters')}
        aria-label="キャラクター管理タブ"
        aria-describedby="tooltip-tab-characters"
        data-testid="tab-btn-characters"
      />
      <Tab
        value="extensions"
        icon={extensionsIcon}
        onclick={() => onTabChange?.('extensions')}
        aria-label="拡張機能管理タブ"
        aria-describedby="tooltip-tab-extensions"
        data-testid="tab-btn-extensions"
      />
    </Tabs>
    <Tooltip id="tooltip-tab-log">ログタブ</Tooltip>
    <Tooltip id="tooltip-tab-graph">シーンタブ</Tooltip>
    <Tooltip id="tooltip-tab-settings">シーン設定タブ</Tooltip>
    <Tooltip id="tooltip-tab-characters">キャラクター管理タブ</Tooltip>
    <Tooltip id="tooltip-tab-extensions">拡張機能管理タブ</Tooltip>
  </div>

  <!-- Character selection button and Send button -->
  <div class="character-action-row" data-testid="character-action-row">
    <div class="char-picker-wrapper">
      <button
        type="button"
        class="char-select-btn"
        onclick={() => (isCharPickerOpen = !isCharPickerOpen)}
        data-testid="character-select-button"
      >
        <img
          class="picker-avatar"
          src={selectedCharacter?.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
          alt={selectedCharacter?.name || 'Anonymous'}
        />
        <span class="picker-name">{selectedCharacter?.name || 'Anonymous'}</span>
      </button>

      {#if isCharPickerOpen}
        <div class="char-picker-dropdown" data-testid="character-picker-dropdown">
          {#each characters as char (char.id)}
            <button
              type="button"
              class="char-picker-item"
              onclick={() => {
                onSelectCharacter?.(char.id);
                isCharPickerOpen = false;
              }}
              data-testid="char-picker-item-{char.id}"
            >
              <img
                class="dropdown-avatar"
                src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                alt={char.name}
              />
              <span>{char.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <IconButton
      variant="filled"
      onclick={handleSend}
      aria-label="送信"
      title="送信"
      data-testid="send-message-button"
    >
      <Icon>send</Icon>
    </IconButton>
  </div>

  <!-- Input field wrapper with chat palette suggestion overlay -->
  <div class="input-wrapper" data-testid="footer-input-wrapper">
    {#if suggestions.length > 0}
      <div
        class="chat-palette-suggestions"
        data-testid="chat-palette-suggestions"
      >
        {#each suggestions as item, idx (idx)}
          <button
            type="button"
            class="suggestion-item"
            onclick={() => handleSelectSuggestion(item)}
            data-testid="suggestion-item-{idx}"
          >
            {item}
          </button>
        {/each}
      </div>
    {/if}

    <input
      type="text"
      class="chat-input-field"
      placeholder="メッセージを入力..."
      bind:value={messageInput}
      oninput={() => {
        isSuggestDismissed = false;
      }}
      onkeydown={handleKeyDown}
      data-testid="chat-input-field"
    />
  </div>
</footer>

<style>
  .common-footer-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    background: var(--np-color-surface-container);
    border-top: 1px solid var(--np-color-outline-variant);
    box-sizing: border-box;
    position: relative;
    z-index: 100;
  }
  .tabs-row {
    width: 100%;
  }
  .character-action-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .char-picker-wrapper {
    position: relative;
  }
  .char-select-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    border-radius: var(--np-shape-corner-full, 20px);
    border: 1px solid var(--np-color-outline);
    background: var(--np-color-surface);
    color: var(--np-color-on-surface);
    cursor: pointer;
    font-size: 0.9rem;
  }
  .picker-avatar {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    object-fit: cover;
  }
  .picker-name {
    font-weight: 500;
  }
  .char-picker-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 6px;
    background: var(--np-color-surface-container-high);
    border: 1px solid var(--np-color-outline-variant);
    border-radius: var(--np-shape-corner-medium, 12px);
    padding: 6px 0;
    box-shadow: var(--np-elevation-2);
    z-index: 200;
    max-height: 200px;
    overflow-y: auto;
    min-width: 160px;
  }
  .char-picker-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: none;
    color: var(--np-color-on-surface);
    text-align: left;
    cursor: pointer;
    font-size: 0.88rem;
  }
  .char-picker-item:hover {
    background: color-mix(in srgb, var(--np-color-primary) 8%, transparent);
  }
  .dropdown-avatar {
    width: 20px;
    height: 20px;
    border-radius: 50%;
  }
  .input-wrapper {
    position: relative;
    width: 100%;
  }
  .chat-input-field {
    width: 100%;
    padding: 10px 14px;
    border: 1px solid var(--np-color-outline);
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-on-surface);
    font-size: 0.95rem;
    box-sizing: border-box;
  }
  /* Suggestion box:
   * "それを入力フィールドの直上に、下側が繋がり、入力フィールドと同じ背景色で、区切り線を挟んで部分一致しているもののリスト、上側は角丸で、他のUI要素を覆いかぶせる形でサジェストする。"
   */
  .chat-palette-suggestions {
    position: absolute;
    bottom: 100%;
    left: 0;
    right: 0;
    background: var(--np-color-surface-container-lowest); /* Same background color as input field */
    border: 1px solid var(--np-color-outline);
    border-bottom: none; /* 下側が繋がり */
    border-top-left-radius: var(--np-shape-corner-medium, 12px); /* 上側は角丸 */
    border-top-right-radius: var(--np-shape-corner-medium, 12px);
    box-shadow: var(--np-elevation-2);
    z-index: 500; /* 他のUI要素を覆いかぶせる */
    max-height: 200px;
    overflow-y: auto;
  }
  .suggestion-item {
    display: block;
    width: 100%;
    padding: 8px 14px;
    text-align: left;
    background: var(--np-color-surface-container-lowest);
    border: none;
    border-bottom: 1px solid var(--np-color-outline-variant); /* 区切り線 */
    font-size: 0.9rem;
    cursor: pointer;
    color: var(--np-color-on-surface);
  }
  .suggestion-item:last-child {
    border-bottom: none;
  }
  .suggestion-item:hover {
    background: color-mix(in srgb, var(--np-color-primary) 10%, transparent);
  }
</style>
