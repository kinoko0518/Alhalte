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
   * Common footer for Control Panel.
   *
   * Features:
   * 1. Tab Selection Button Group (Size L, standard, smooth M3 Expressive expand animation on select).
   * 2. Character Selector (Filled tonal Button with M3 Menu popover) & Right-aligned Send Button (Filled Button).
   * 3. Center-aligned 8-dice buttons row (Standard IconButtons for 1D2, 1D4, 1D6, 1D8, 1D10, 1D12, 1D20, 1D100) with Tooltips.
   * 4. Multiline Outlined TextField (minLines=4, maxLines=10, rows=4) with chat palette partial-match suggestion overlay.
   * 5. Generast AST generation on message submission (via extension pipeline if active, or text node fallback).
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通フッター`
   * - ARCHITECTURE.md: `# 拡張機能 -> ## 拡張機能ABI`
   * - GitHub Issue #54: 共通フッターに関する提言
   */
  import {
    Button,
    ButtonGroup,
    IconButton,
    Menu,
    MenuItem,
    TextField,
    Tooltip,
  } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character, ExtensionItem } from '../../types/index.js';
  import {
    findChatPaletteMatches,
    replaceChatPaletteMatch,
  } from '../../utils/chatPalette.js';
  import {
    buildGenerastFromText,
    createTextGenerastAST,
    type GenerastAST,
  } from '../../utils/generast.js';
  import { ExtensionPipeline } from '../../wasm/runner.js';

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
    extensions = [],
    extensionPipeline = new ExtensionPipeline(),
    selectedCharacterId,
    showDicePicker = $bindable(true),
    isViewer = false,
    onTabChange,
    onSelectCharacter,
    onSendMessage,
    onSendGenerast,
    onToggleDicePicker,
  }: {
    activeTab?: ActiveTab;
    characters?: Character[];
    extensions?: ExtensionItem[];
    extensionPipeline?: ExtensionPipeline;
    selectedCharacterId?: string;
    showDicePicker?: boolean;
    isViewer?: boolean;
    onTabChange?: (tab: ActiveTab) => void;
    onSelectCharacter?: (charId: string) => void;
    onSendMessage?: (text: string, characterId?: string, generast?: GenerastAST) => void;
    onSendGenerast?: (generast: GenerastAST, characterId?: string, rawText?: string) => void;
    onToggleDicePicker?: (show: boolean) => void;
  } = $props();

  let messageInput = $state('');
  let isCharPickerOpen = $state(false);
  let isMoreMenuOpen = $state(false);
  let isSuggestDismissed = $state(false);
  let textareaElement = $state<HTMLTextAreaElement | null>(null);
  let charBtnElement = $state<HTMLElement | undefined>(undefined);
  let charPickerWrapper = $state<HTMLElement | null>(null);
  let moreBtnElement = $state<HTMLElement | undefined>(undefined);
  let moreMenuWrapper = $state<HTMLElement | null>(null);

  const tabItems: Array<{ id: ActiveTab; icon: string; label: string; fullLabel: string }> = [
    { id: 'log', icon: 'forum', label: 'ログ', fullLabel: 'ログタブ' },
    { id: 'graph', icon: 'account_tree', label: 'シーングラフ', fullLabel: 'シーングラフタブ' },
    { id: 'settings', icon: 'tune', label: 'シーン管理', fullLabel: 'シーン管理タブ' },
    { id: 'characters', icon: 'group', label: 'キャラクター', fullLabel: 'キャラクター管理タブ' },
    { id: 'extensions', icon: 'extension', label: '拡張機能', fullLabel: '拡張機能管理タブ' },
  ];

  const diceList = ['1D2', '1D4', '1D6', '1D8', '1D10', '1D12', '1D20', '1D100'] as const;

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

  function handleTabClick(tabId: ActiveTab) {
    onTabChange?.(tabId);
  }

  function handleSelectSuggestion(suggestionLine: string) {
    messageInput = replaceChatPaletteMatch(messageInput, messageInput, suggestionLine);
    isSuggestDismissed = true;
    if (textareaElement) {
      textareaElement.focus();
    }
  }

  function handleInsertDice(dice: string) {
    if (isViewer) return;
    if (textareaElement) {
      const start = textareaElement.selectionStart ?? messageInput.length;
      const end = textareaElement.selectionEnd ?? messageInput.length;
      messageInput = messageInput.slice(0, start) + dice + messageInput.slice(end);
      isSuggestDismissed = false;
      const nextPos = start + dice.length;
      queueMicrotask(() => {
        if (textareaElement) {
          textareaElement.focus();
          textareaElement.setSelectionRange(nextPos, nextPos);
        }
      });
    } else {
      messageInput += dice;
      isSuggestDismissed = false;
    }
  }

  function handleSend() {
    if (isViewer) return;
    const rawText = messageInput.trim();
    if (!rawText) return;

    const charId = selectedCharacter?.id;
    const activeExtensions = extensions.filter((ext) => ext.enabled);

    if (activeExtensions.length === 0) {
      const generastAst = createTextGenerastAST(rawText);
      onSendMessage?.(rawText, charId, generastAst);
      onSendGenerast?.(generastAst, charId, rawText);
      messageInput = '';
      isSuggestDismissed = false;
      return;
    }

    // Execute pipeline asynchronously when extensions are enabled
    buildGenerastFromText(rawText, activeExtensions, extensionPipeline, charId).then((generastAst) => {
      onSendMessage?.(rawText, charId, generastAst);
      onSendGenerast?.(generastAst, charId, rawText);
    });

    messageInput = '';
    isSuggestDismissed = false;
  }


  function handleKeyDown(e: KeyboardEvent) {
    if (isViewer) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function dismissSuggestions() {
    isSuggestDismissed = true;
  }

  function handleWindowClick(e: MouseEvent) {
    dismissSuggestions();
    if (isCharPickerOpen && charPickerWrapper && !charPickerWrapper.contains(e.target as Node)) {
      isCharPickerOpen = false;
    }
    if (isMoreMenuOpen && moreMenuWrapper && !moreMenuWrapper.contains(e.target as Node)) {
      isMoreMenuOpen = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

{#snippet diceSvg(sides: string)}
  {#if sides === '1D2'}
    <!-- 1D2: Circle with divider and '2' -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="9" />
      <line x1="12" y1="3" x2="12" y2="21" />
      <text x="7.5" y="14.5" font-size="7" font-weight="bold" fill="currentColor" stroke="none">1</text>
      <text x="13" y="14.5" font-size="7" font-weight="bold" fill="currentColor" stroke="none">2</text>
    </svg>
  {:else if sides === '1D4'}
    <!-- 1D4: Tetrahedron (triangle) -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="12,3 21,19 3,19" />
      <line x1="12" y1="3" x2="12" y2="13.5" />
      <line x1="3" y1="19" x2="12" y2="13.5" />
      <line x1="21" y1="19" x2="12" y2="13.5" />
    </svg>
  {:else if sides === '1D6'}
    <!-- 1D6: Cube isometric / pip dice -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="4" y="4" width="16" height="16" rx="2.5" />
      <circle cx="8" cy="8" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="16" cy="16" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="16" cy="8" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="8" cy="16" r="1.5" fill="currentColor" stroke="none" />
    </svg>
  {:else if sides === '1D8'}
    <!-- 1D8: Octahedron (diamond) -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="12,2 21,12 12,22 3,12" />
      <line x1="12" y1="2" x2="12" y2="22" />
      <line x1="3" y1="12" x2="21" y2="12" />
    </svg>
  {:else if sides === '1D10'}
    <!-- 1D10: Trapezohedron -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="12,2 20.5,8 17.5,21.5 6.5,21.5 3.5,8" />
      <line x1="12" y1="2" x2="12" y2="13.5" />
      <line x1="20.5" y1="8" x2="12" y2="13.5" />
      <line x1="17.5" y1="21.5" x2="12" y2="13.5" />
      <line x1="6.5" y1="21.5" x2="12" y2="13.5" />
      <line x1="3.5" y1="8" x2="12" y2="13.5" />
    </svg>
  {:else if sides === '1D12'}
    <!-- 1D12: Dodecahedron (pentagon face) -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="12,2 21,8.5 17.5,20 6.5,20 3,8.5" />
      <polygon points="12,6.5 17.5,10.5 15.5,17 8.5,17 6.5,10.5" />
      <line x1="12" y1="2" x2="12" y2="6.5" />
      <line x1="21" y1="8.5" x2="17.5" y2="10.5" />
      <line x1="17.5" y1="20" x2="15.5" y2="17" />
      <line x1="6.5" y1="20" x2="8.5" y2="17" />
      <line x1="3" y1="8.5" x2="6.5" y2="10.5" />
    </svg>
  {:else if sides === '1D20'}
    <!-- 1D20: Icosahedron -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="12,2 21,7 21,17 12,22 3,17 3,7" />
      <polygon points="12,5.5 18.5,15.5 5.5,15.5" />
      <line x1="12" y1="2" x2="12" y2="5.5" />
      <line x1="3" y1="7" x2="5.5" y2="15.5" />
      <line x1="21" y1="7" x2="18.5" y2="15.5" />
      <line x1="12" y1="22" x2="5.5" y2="15.5" />
      <line x1="12" y1="22" x2="18.5" y2="15.5" />
    </svg>
  {:else}
    <!-- 1D100: Percentile / 100 die -->
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="9.5" />
      <text x="12" y="15" font-size="7.5" font-weight="bold" text-anchor="middle" fill="currentColor" stroke="none">100</text>
    </svg>
  {/if}
{/snippet}

<footer
  class="common-footer-container"
  data-testid="common-footer"
>
  <!-- 1. Tab Selection Button Group (smooth horizontal expand on select) -->
  <div class="tabs-row np-tabs" data-testid="tab-buttons-row">
    <ButtonGroup
      variant="standard"
      size="s"
      selection="single"
      value={activeTab}
      expandedRatio={0}
      class="tab-button-group"
    >
      {#each tabItems as tab (tab.id)}
        {@const isSelected = activeTab === tab.id}
        <Button
          size="s"
          variant={isSelected ? 'filled' : 'outlined'}
          class={['tab-group-btn np-tab', isSelected && 'selected']}
          onclick={() => handleTabClick(tab.id)}
          aria-label={tab.fullLabel}
          aria-describedby={`tooltip-tab-${tab.id}`}
          data-testid={`tab-btn-${tab.id}`}
        >
          {#snippet start()}
            <Icon>{tab.icon}</Icon>
          {/snippet}
          <span class={['tab-btn-label tab-label-text', isSelected ? 'expanded' : 'collapsed']}>
            {tab.label}
          </span>
        </Button>
        <Tooltip id={`tooltip-tab-${tab.id}`}>{tab.fullLabel}</Tooltip>
      {/each}
    </ButtonGroup>
  </div>

  <!-- Character selection button & Right-aligned Send button -->
  <div class="character-action-row" data-testid="character-action-row">
    <div class="char-picker-container" bind:this={charPickerWrapper}>
      <Button
        variant="tonal"
        size="s"
        class="character-select-tonal-btn"
        onclick={() => (isCharPickerOpen = !isCharPickerOpen)}
        aria-haspopup="menu"
        aria-expanded={isCharPickerOpen}
        bind:element={charBtnElement}
        data-testid="character-select-button"
      >
        <span class="char-btn-content">
          <img
            class="picker-avatar"
            src={selectedCharacter?.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
            alt={selectedCharacter?.name || 'Anonymous'}
          />
          <span class="picker-name">{selectedCharacter?.name || 'Anonymous'}</span>
          <Icon>arrow_drop_down</Icon>
        </span>
      </Button>

      {#if isCharPickerOpen}
        <Menu
          bind:open={isCharPickerOpen}
          anchor={charBtnElement}
          class="char-picker-menu"
          data-testid="character-picker-dropdown"
        >
          {#each characters as char (char.id)}
            <MenuItem
              selected={char.id === selectedCharacter?.id}
              onclick={() => {
                onSelectCharacter?.(char.id);
                isCharPickerOpen = false;
              }}
              data-testid={`char-picker-item-${char.id}`}
            >
              <div class="char-menu-item-inner">
                <img
                  class="dropdown-avatar"
                  src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
                  alt={char.name}
                />
                <span class="dropdown-char-name">{char.name}</span>
              </div>
            </MenuItem>
          {/each}
        </Menu>
      {/if}
    </div>

    <!-- Right-aligned Send Button and More options Button -->
    <div class="send-action-wrapper">
      <Button
        variant="filled"
        size="s"
        class="send-btn"
        onclick={handleSend}
        disabled={isViewer}
        aria-label="送信"
        title={isViewer ? '閲覧者は送信できません' : '送信'}
        data-testid="send-message-button"
      >
        {#snippet start()}
          <Icon>send</Icon>
        {/snippet}
        送信
      </Button>

      <div class="more-menu-container" bind:this={moreMenuWrapper}>
        <IconButton
          variant="text"
          size="s"
          class="more-btn"
          bind:element={moreBtnElement}
          onclick={() => (isMoreMenuOpen = !isMoreMenuOpen)}
          aria-label="その他"
          title="その他"
          aria-haspopup="menu"
          aria-expanded={isMoreMenuOpen}
          aria-describedby="tooltip-footer-more"
          data-testid="footer-more-button"
        >
          <Icon>more_vert</Icon>
        </IconButton>
        <Tooltip id="tooltip-footer-more">その他</Tooltip>

        {#if isMoreMenuOpen}
          <Menu
            bind:open={isMoreMenuOpen}
            anchor={moreBtnElement}
            class="footer-more-menu"
            data-testid="footer-more-menu"
          >
            <MenuItem
              onclick={() => {
                showDicePicker = !showDicePicker;
                onToggleDicePicker?.(showDicePicker);
                isMoreMenuOpen = false;
              }}
              data-testid="toggle-dice-picker-menu-item"
            >
              {#snippet start()}
                <Icon>{showDicePicker ? 'visibility_off' : 'visibility'}</Icon>
              {/snippet}
              {showDicePicker ? 'ダイスピッカーを非表示' : 'ダイスピッカーを表示'}
            </MenuItem>
          </Menu>
        {/if}
      </div>
    </div>
  </div>

  <!-- Dice buttons row (Center-aligned, horizontally scrollable) -->
  {#if showDicePicker}
    <div class="dice-buttons-row" data-testid="dice-buttons-row">
      {#each diceList as dice (dice)}
        <IconButton
          variant="text"
          size="m"
          onclick={() => handleInsertDice(dice)}
          disabled={isViewer}
          aria-label={dice}
          aria-describedby={`tooltip-dice-${dice.toLowerCase()}`}
          data-testid={`dice-btn-${dice.toLowerCase()}`}
          class="dice-icon-btn"
        >
          {@render diceSvg(dice)}
        </IconButton>
        <Tooltip id={`tooltip-dice-${dice.toLowerCase()}`}>{dice}</Tooltip>
      {/each}
    </div>
  {/if}

  <!-- Input field wrapper with chat palette suggestion overlay -->
  <div class="input-wrapper" data-testid="footer-input-wrapper">
    {#if suggestions.length > 0 && !isViewer}
      <div
        class="chat-palette-suggestions"
        data-testid="chat-palette-suggestions"
      >
        {#each suggestions as item, idx (idx)}
          <button
            type="button"
            class="suggestion-item"
            onclick={() => handleSelectSuggestion(item)}
            data-testid={`suggestion-item-${idx}`}
          >
            {item}
          </button>
        {/each}
      </div>
    {/if}

    <TextField
      variant="outlined"
      type="textarea"
      minLines={4}
      maxLines={10}
      rows={4}
      disabled={isViewer}
      placeholder={isViewer ? '閲覧者モードのためメッセージは送信できません' : 'メッセージを入力...'}
      bind:value={messageInput}
      bind:inputElement={textareaElement}
      oninput={() => {
        isSuggestDismissed = false;
      }}
      onkeydown={handleKeyDown}
      data-testid="chat-input-field"
      class="chat-text-field"
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
    flex-shrink: 0;
  }

  /* 1. Tabs Button Group Row */
  .tabs-row {
    width: 100%;
    overflow-x: auto;
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  .tabs-row::-webkit-scrollbar {
    display: none;
  }
  .tabs-row :global(.tab-button-group) {
    display: flex;
    width: 100%;
    gap: 6px;
    align-items: center;
  }
  .tabs-row :global(.tab-group-btn) {
    flex-grow: 0;
    flex-shrink: 0;
    flex-basis: auto;
    transition: flex-grow var(--np-motion-expressive-fast-spatial, 200ms ease),
                background-color var(--np-motion-expressive-fast-effects, 200ms ease),
                color var(--np-motion-expressive-fast-effects, 200ms ease);
    overflow: hidden;
    white-space: nowrap;
    will-change: flex-grow;
  }
  .tabs-row :global(.tab-group-btn.selected) {
    flex-grow: 1;
  }
  .tab-btn-label,
  :global(.tab-label-text) {
    display: inline-block;
    transition: max-width var(--np-motion-expressive-fast-spatial, 200ms ease),
                opacity var(--np-motion-expressive-fast-effects, 200ms ease),
                margin var(--np-motion-expressive-fast-effects, 200ms ease);
    overflow: hidden;
    white-space: nowrap;
    vertical-align: middle;
    will-change: max-width, opacity;
  }
  .tab-btn-label.collapsed,
  :global(.tab-label-text.collapsed) {
    max-width: 0;
    opacity: 0;
    margin: 0;
  }
  .tab-btn-label.expanded,
  :global(.tab-label-text.expanded) {
    max-width: 120px;
    opacity: 1;
    margin-left: 6px;
  }

  /* 2. Character selection & Send button */
  .character-action-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
  }
  .char-picker-container {
    position: relative;
    display: inline-flex;
  }
  .char-btn-content {
    display: inline-flex;
    align-items: center;
    gap: 8px;
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
  .char-menu-item-inner {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
  }
  .dropdown-avatar {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    object-fit: cover;
  }
  .dropdown-char-name {
    font-size: 0.9rem;
  }
  .send-action-wrapper {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .more-menu-container {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  /* 3. Dice buttons row (horizontal scroll on narrow width) */
  .dice-buttons-row {
    display: flex;
    justify-content: safe center;
    align-items: center;
    flex-wrap: nowrap;
    overflow-x: auto;
    overflow-y: hidden;
    gap: 6px;
    width: 100%;
    padding: 2px 0;
    box-sizing: border-box;
    scrollbar-width: thin;
    scrollbar-color: var(--np-color-outline-variant) transparent;
  }
  .dice-buttons-row::-webkit-scrollbar {
    height: 4px;
  }
  .dice-buttons-row::-webkit-scrollbar-track {
    background: transparent;
  }
  .dice-buttons-row::-webkit-scrollbar-thumb {
    background: var(--np-color-outline-variant);
    border-radius: 4px;
  }
  .dice-buttons-row :global(.dice-icon-btn) {
    flex: 0 0 auto;
    color: var(--np-color-on-surface-variant);
    transition: color var(--np-motion-expressive-fast-effects, 200ms ease),
                transform var(--np-motion-expressive-fast-spatial, 200ms ease);
  }
  .dice-buttons-row :global(.dice-icon-btn:hover) {
    color: var(--np-color-primary);
    transform: scale(1.1);
  }

  /* 4. Input wrapper and Suggestion overlay */
  .input-wrapper {
    position: relative;
    width: 100%;
  }
  .input-wrapper :global(.chat-text-field) {
    width: 100%;
  }
  .input-wrapper :global(.chat-text-field textarea) {
    resize: none;
  }

  /* Suggestion box:
   * "それを入力フィールドの直上に、下側が繋がり、入力フィールドと同じ背景色で、区切り線を挟んで部分一致しているもののリスト、上側は角丸で、他のUI要素を覆いかぶせる形でサジェストする。"
   */
  .chat-palette-suggestions {
    position: absolute;
    bottom: 100%;
    left: 0;
    right: 0;
    background: var(--np-color-surface-container-lowest);
    border: 1px solid var(--np-color-outline);
    border-bottom: none;
    border-top-left-radius: var(--np-shape-corner-medium, 12px);
    border-top-right-radius: var(--np-shape-corner-medium, 12px);
    box-shadow: var(--np-elevation-2);
    z-index: 500;
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
    border-bottom: 1px solid var(--np-color-outline-variant);
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
