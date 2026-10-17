import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  findChatPaletteMatches,
  parseChatPaletteLines,
  replaceChatPaletteMatch,
} from '../src/utils/chatPalette.js';
import CommonFooter from '../src/components/Control/CommonFooter.svelte';
import type { Character } from '../src/types/index.js';

describe('Chat Palette Suggestion & Common Footer', () => {
  const samplePalette = `
1D100<=50 【目星】
1D100<=70 【聞き耳】
1D100<=60 【図書館】
1D6+1D4 ダメージボーナス
`;

  // spec: alhalte-argenterie/chat-palette 部分一致サジェスト
  it('identifies partial matches from multiline chat palette', () => {
    const lines = parseChatPaletteLines(samplePalette);
    expect(lines).toHaveLength(4);

    const matches1 = findChatPaletteMatches('目星', samplePalette);
    expect(matches1).toEqual(['1D100<=50 【目星】']);

    const matches2 = findChatPaletteMatches('1D100', samplePalette);
    expect(matches2).toHaveLength(3);

    const emptyMatches = findChatPaletteMatches('', samplePalette);
    expect(emptyMatches).toEqual([]);
  });

  // spec: alhalte-argenterie/chat-palette サジェスト置換
  it('replaces matched portion with the chosen full chat palette line', () => {
    const currentInput = '目星';
    const chosenLine = '1D100<=50 【目星】';
    const replaced = replaceChatPaletteMatch(currentInput, '目星', chosenLine);
    expect(replaced).toBe('1D100<=50 【目星】');

    const typedPrefix = 'cc 1d100';
    const replaced2 = replaceChatPaletteMatch(typedPrefix, '1d100', '1D100<=70 【聞き耳】');
    expect(replaced2).toBe('cc 1D100<=70 【聞き耳】');
  });

  // spec: alhalte-argenterie/chat-palette CommonFooter コンポーネントサジェストUI
  it('renders CommonFooter with 6 tabs and handles suggestions overlay and tap replacement', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testCharacters: Character[] = [
      {
        id: 'char-1',
        name: '探偵',
        initiative: 10,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: samplePalette,
        hideStatus: false,
      },
    ];

    let sentMessage: string | null = null;
    let switchedTab: string | null = null;

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: testCharacters,
        selectedCharacterId: 'char-1',
        onTabChange: (tab) => {
          switchedTab = tab;
        },
        onSendMessage: (msg) => {
          sentMessage = msg;
        },
      },
    });

    // Check 5 M3 Tab buttons with Material Symbols icons and Tooltips (Issue #30 & Issue #33)
    expect(target.querySelector('.np-tabs')).toBeTruthy();
    const expectedTabs = [
      { id: 'log', icon: 'forum', label: 'ログタブ' },
      { id: 'graph', icon: 'account_tree', label: 'シーンタブ' },
      { id: 'settings', icon: 'tune', label: 'シーン設定タブ' },
      { id: 'characters', icon: 'group', label: 'キャラクター管理タブ' },
      { id: 'extensions', icon: 'extension', label: '拡張機能管理タブ' },
    ];
    for (const tab of expectedTabs) {
      const tabBtn = target.querySelector<HTMLButtonElement>(`[data-testid="tab-btn-${tab.id}"]`);
      expect(tabBtn).toBeTruthy();
      expect(tabBtn?.classList.contains('np-tab')).toBe(true);
      expect(tabBtn?.getAttribute('aria-label')).toBe(tab.label);
      expect(tabBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe(tab.icon);

      const tooltip = target.querySelector(`#tooltip-tab-${tab.id}`);
      expect(tooltip).toBeTruthy();
      expect(tooltip?.textContent?.trim()).toBe(tab.label);
    }
    expect(target.querySelector('[data-testid="tab-btn-scenes"]')).toBeNull();

    // Click tab button
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]')?.click();
    expect(switchedTab).toBe('graph');

    // Type partial match into input
    const input = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]');
    expect(input).toBeTruthy();
    if (input) {
      input.value = '目星';
      input.dispatchEvent(new Event('input', { bubbles: true }));
    }

    // Wait for Svelte reactivity update
    await new Promise((r) => setTimeout(r, 20));

    // Suggestions container should appear directly above input
    const suggestBox = target.querySelector('[data-testid="chat-palette-suggestions"]');
    expect(suggestBox).toBeTruthy();

    const item0 = target.querySelector<HTMLButtonElement>('[data-testid="suggestion-item-0"]');
    expect(item0?.textContent?.trim()).toBe('1D100<=50 【目星】');

    // Tap suggestion to replace
    item0?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(input?.value).toBe('1D100<=50 【目星】');

    // Send IconButton with 'send' icon (Issue #33)
    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(sendBtn).toBeTruthy();
    expect(sendBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(sendBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('send');
    sendBtn?.click();
    expect(sentMessage).toBe('1D100<=50 【目星】');

    unmount(instance);
    target.remove();
  });
});
