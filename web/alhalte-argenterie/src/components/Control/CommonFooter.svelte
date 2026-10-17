<script lang="ts">
  /**
   * Common footer for Control Panel with 6 tab buttons, character selector,
   * send button, input field, and chat palette partial-match suggestion overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通フッター`
   */
  import { Button } from 'noph-ui';
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

  function handleInputChange(e: Event) {
    messageInput = (e.target as HTMLInputElement).value;
    isSuggestDismissed = false;
  }

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

<footer
  class="common-footer-container"
  data-testid="common-footer"
>
  <!-- 5 tab buttons (Scene Management merged into Scene Tab per Issue #30) -->
  <div class="tabs-row" data-testid="tab-buttons-row">
    <button
      type="button"
      class="tab-btn"
      class:active={activeTab === 'log'}
      onclick={() => onTabChange?.('log')}
      data-testid="tab-btn-log"
    >
      ログタブ
    </button>
    <button
      type="button"
      class="tab-btn"
      class:active={activeTab === 'graph'}
      onclick={() => onTabChange?.('graph')}
      data-testid="tab-btn-graph"
    >
      シーンタブ
    </button>
    <button
      type="button"
      class="tab-btn"
      class:active={activeTab === 'settings'}
      onclick={() => onTabChange?.('settings')}
      data-testid="tab-btn-settings"
    >
      シーン設定タブ
    </button>
    <button
      type="button"
      class="tab-btn"
      class:active={activeTab === 'characters'}
      onclick={() => onTabChange?.('characters')}
      data-testid="tab-btn-characters"
    >
      キャラクター管理タブ
    </button>
    <button
      type="button"
      class="tab-btn"
      class:active={activeTab === 'extensions'}
      onclick={() => onTabChange?.('extensions')}
      data-testid="tab-btn-extensions"
    >
      拡張機能管理タブ
    </button>
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

    <Button variant="filled" onclick={handleSend} data-testid="send-message-button">
      送信
    </Button>
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
    background: var(--md-sys-color-surface-container, #f3edf7);
    border-top: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
    box-sizing: border-box;
    position: relative;
    z-index: 100;
  }
  .tabs-row {
    display: flex;
    overflow-x: auto;
    gap: 6px;
    padding-bottom: 4px;
    scrollbar-width: thin;
  }
  .tab-btn {
    flex-shrink: 0;
    padding: 6px 12px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--md-sys-color-on-surface-variant, #49454f);
    font-size: 0.82rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }
  .tab-btn:hover {
    background: rgba(103, 80, 164, 0.08);
  }
  .tab-btn.active {
    background: var(--md-sys-color-secondary-container, #e8def8);
    color: var(--md-sys-color-on-secondary-container, #1d192b);
    font-weight: 600;
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
    border-radius: 20px;
    border: 1px solid var(--md-sys-color-outline, #79747e);
    background: var(--md-sys-color-surface, #fff);
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
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    border: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
    border-radius: 12px;
    padding: 6px 0;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
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
    text-align: left;
    cursor: pointer;
    font-size: 0.88rem;
  }
  .char-picker-item:hover {
    background: rgba(103, 80, 164, 0.08);
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
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-radius: 12px;
    background: #ffffff;
    color: #1b1b1f;
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
    background: #ffffff; /* Same background color as input field */
    border: 1px solid var(--md-sys-color-outline, #79747e);
    border-bottom: none; /* 下側が繋がり */
    border-top-left-radius: 12px; /* 上側は角丸 */
    border-top-right-radius: 12px;
    box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.15);
    z-index: 500; /* 他のUI要素を覆いかぶせる */
    max-height: 200px;
    overflow-y: auto;
  }
  .suggestion-item {
    display: block;
    width: 100%;
    padding: 8px 14px;
    text-align: left;
    background: #ffffff;
    border: none;
    border-bottom: 1px solid var(--md-sys-color-outline-variant, #e0e0e0); /* 区切り線 */
    font-size: 0.9rem;
    cursor: pointer;
    color: #1b1b1f;
  }
  .suggestion-item:last-child {
    border-bottom: none;
  }
  .suggestion-item:hover {
    background: rgba(103, 80, 164, 0.1);
  }
</style>
