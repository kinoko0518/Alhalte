/**
 * Unit & Integration tests for Character Management & Character Edit Modal Hoisting.
 *
 * References:
 * - Issue #52: アイコン画像選択UIの分離（単一アイコン表示、ホバーズーム、Carousel不要）
 * - Issue #60: キャラクター編集モーダルの全画面化（ビューポート最前面スクリム上の中央表示）
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import CharacterEditModal from '../src/components/Control/CharacterEditModal.svelte';
import App from '../src/components/App.svelte';
import { clearAssets, addAsset } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

describe('Character Management & CharacterEditModal Hoisting (Issue #52, #60)', () => {
  const dummyRoom: VisitedRoom = {
    id: 'room-char-mgmt-test',
    name: 'キャラ管理検証部屋',
    url: 'https://alhalte.example/rooms/room-char-mgmt-test#key',
    lastVisited: Date.now(),
    isAdmin: true,
  };

  const scene1: Scene = {
    id: 'scene-1',
    name: 'メインシーン',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: ['char-hero'],
  };

  const heroChar: Character = {
    id: 'char-hero',
    name: '勇者ロラン',
    initiative: 15,
    description: '前衛の騎士。誇り高く仲間思いの剣士。',
    reference: 'https://example.com/character/hero',
    statuses: [{ label: 'HP', current: 30, max: 30 }],
    parameters: [{ label: 'STR', value: 16 }],
    chatPalette: '1D100<=80 【近接攻撃】',
    hideStatus: false,
    placedAsPiece: false,
  };

  beforeEach(() => {
    clearAssets();
    document.body.innerHTML = '';
  });

  // spec: Issue #52 キャラクター編集モーダル内にはCarouselが存在せず、単一アイコンとホバーズームであること
  it('renders CharacterEditModal without Carousel, supports hover zoom on avatar, and edits parameters', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let savedChar: Character | null = null;
    let closed = false;

    const instance = mount(CharacterEditModal, {
      target,
      props: {
        character: heroChar,
        isAdmin: true,
        onSave: (c) => {
          savedChar = c;
        },
        onClose: () => {
          closed = true;
        },
      },
    });

    // 1. Verify NO Carousel is present in character edit modal (Issue #52: Carousel は不要)
    expect(target.querySelector('.np-carousel')).toBeNull();
    expect(target.querySelector('[data-testid="char-image-carousel"]')).toBeNull();

    // 2. Avatar picker is present with hover zoom
    const avatarPicker = target.querySelector<HTMLButtonElement>('[data-testid="edit-avatar-picker"]');
    expect(avatarPicker).toBeTruthy();

    const avatarImg = target.querySelector<HTMLImageElement>('[data-testid="edit-avatar-img"]');
    expect(avatarImg).toBeTruthy();
    expect(avatarImg?.style.transform).toBe('scale(1)');

    avatarPicker?.dispatchEvent(new MouseEvent('mouseenter'));
    await new Promise((r) => setTimeout(r, 20));
    expect(avatarImg?.style.transform).toBe('scale(1.1)');

    avatarPicker?.dispatchEvent(new MouseEvent('mouseleave'));
    await new Promise((r) => setTimeout(r, 20));
    expect(avatarImg?.style.transform).toBe('scale(1)');

    // 3. Edit name and add status/param rows
    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="edit-name-input"]');
    expect(nameInput).toBeTruthy();
    if (nameInput) {
      nameInput.value = '覚醒勇者ロラン';
      nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }

    // Add status row
    target.querySelector<HTMLButtonElement>('[data-testid="add-status-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const statusList = target.querySelector('[data-testid="edit-status-list"]');
    expect(statusList?.children.length).toBe(2);

    // Save
    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(savedChar?.name).toBe('覚醒勇者ロラン');
    expect(savedChar?.statuses.length).toBe(2);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #60 Appコンポーネントにおけるキャラクター編集モーダルの全画面ビューポート最前面化
  it('hoists CharacterEditModal to root viewport level outside control panel and side sheet with full-screen dark scrim in App.svelte', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1],
        initialCharacters: [heroChar],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Open Character tab
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Click character card to edit
    const charCard = target.querySelector<HTMLElement>(`[data-testid="character-card-${heroChar.id}"]`);
    expect(charCard).toBeTruthy();
    charCard?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify modal is outside control panel and outside landscape side sheet
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const sideSheet = target.querySelector('[data-testid="landscape-side-sheet"]');
    const rootWrapper = target.querySelector('[data-testid="root-character-edit-wrapper"]');
    const editPanel = target.querySelector('[data-testid="character-edit-panel"]');

    expect(rootWrapper).toBeTruthy();
    expect(editPanel).toBeTruthy();
    expect(controlPanel?.contains(editPanel)).toBe(false);
    expect(sideSheet?.contains(editPanel)).toBe(false);

    // Scrim overlay check
    expect(editPanel?.classList.contains('character-edit-overlay')).toBe(true);
    expect(editPanel?.classList.contains('edit-panel-overlay')).toBe(true);

    // Click cancel button closes modal
    target.querySelector<HTMLButtonElement>('[data-testid="cancel-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: Issue #129 キャラクター編集モーダルのM3 Expressive Switch (hideStatus, isNarrative)、M3 Divider、説明文、縦スクロール
  it('renders CharacterEditModal with M3 Switches (hideStatus, isNarrative), Dividers, section descriptions, and scrollable container', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let savedChar: Character | null = null;

    const instance = mount(CharacterEditModal, {
      target,
      props: {
        character: heroChar,
        isAdmin: true,
        onSave: (c) => {
          savedChar = c;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // 1. Section description texts
    const statusDesc = target.querySelector('[data-testid="status-section-desc"]');
    expect(statusDesc).toBeTruthy();
    expect(statusDesc?.textContent).toBe('体力などのゲーム中に頻繁に更新される値です。');

    const paramDesc = target.querySelector('[data-testid="param-section-desc"]');
    expect(paramDesc).toBeTruthy();
    expect(paramDesc?.textContent).toBe('筋力、知力などゲーム中めったに更新されない値です。');

    // 2. M3 Dividers (.np-divider)
    const dividers = target.querySelectorAll('.np-divider');
    expect(dividers.length).toBeGreaterThanOrEqual(6);

    // Verify sub-sections contain divider under header & description
    const statusSection = target.querySelector('.sub-section');
    expect(statusSection?.querySelector('.np-divider')).toBeTruthy();

    // 3. M3 Switch for "ステータスを秘匿"
    const hideStatusInput = target.querySelector<HTMLInputElement>('[data-testid="edit-hide-status-toggle"]');
    expect(hideStatusInput).toBeTruthy();
    expect(hideStatusInput?.getAttribute('role')).toBe('switch');
    expect(hideStatusInput?.getAttribute('aria-label')).toBe('ステータスを秘匿');
    expect(hideStatusInput?.closest('.np-switch')).toBeTruthy();
    expect(hideStatusInput?.checked).toBe(false);

    // Toggle hide status
    hideStatusInput?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(hideStatusInput?.checked).toBe(true);

    // 4. M3 Switch for "地の文" (narrative toggle)
    const narrativeInput = target.querySelector<HTMLInputElement>('[data-testid="edit-narrative-toggle"]');
    expect(narrativeInput).toBeTruthy();
    expect(narrativeInput?.getAttribute('role')).toBe('switch');
    expect(narrativeInput?.getAttribute('aria-label')).toBe('地の文');
    expect(narrativeInput?.closest('.np-switch')).toBeTruthy();
    expect(narrativeInput?.checked).toBe(false);

    // Toggle narrative
    narrativeInput?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(narrativeInput?.checked).toBe(true);

    // 5. Scrollable container structure
    const overlay = target.querySelector<HTMLElement>('[data-testid="character-edit-panel"]');
    expect(overlay).toBeTruthy();
    expect(overlay?.style.overflowY).toBe('auto');
    expect(overlay?.style.overscrollBehavior).toBe('contain');

    const dialogContent = target.querySelector<HTMLElement>('.edit-dialog-content');
    expect(dialogContent).toBeTruthy();
    expect(dialogContent?.style.overflowY).toBe('auto');
    expect(dialogContent?.style.maxHeight).toBe('85vh');
    expect(dialogContent?.style.overscrollBehavior).toBe('contain');

    // 6. Save and verify updated properties
    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(savedChar?.hideStatus).toBe(true);
    expect(savedChar?.isNarrative).toBe(true);

    unmount(instance);
    target.remove();
  });
});
