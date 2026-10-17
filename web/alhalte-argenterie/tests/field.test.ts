import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  applyPan,
  applyPinch,
  applyZoom,
  chunkStatusesInPairs,
  formatStatusItem,
  sortCharactersByInitiative,
} from '../src/utils/field.js';
import FieldPanel from '../src/components/Field/FieldPanel.svelte';
import type { Character, Scene, ViewTransform } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

describe('Field Panel & Initiatives', () => {
  // spec: alhalte-argenterie/field パン/ズーム/ピンチ計算
  it('calculates pan, zoom, and pinch transformations correctly', () => {
    const initial: ViewTransform = { x: 100, y: 50, scale: 1.0 };

    // Pan
    const panned = applyPan(initial, 20, -10);
    expect(panned).toEqual({ x: 120, y: 40, scale: 1.0 });

    // Zoom around center
    const zoomed = applyZoom(initial, 2.0, { x: 100, y: 50 });
    expect(zoomed.scale).toBe(2.0);
    expect(zoomed.x).toBe(100);
    expect(zoomed.y).toBe(50);

    // Zoom bounds clamping
    const clamped = applyZoom(initial, 100, undefined, { min: 0.5, max: 5.0 });
    expect(clamped.scale).toBe(5.0);

    // Pinch calculation
    const pinched = applyPinch(initial, 100, 150);
    expect(pinched.scale).toBe(1.5);
  });

  // spec: alhalte-argenterie/field イニシアチブ降順ソート
  it('sorts characters in descending order of initiative', () => {
    const chars: Character[] = [
      {
        id: 'c1',
        name: 'Fighter',
        initiative: 12,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
      {
        id: 'c2',
        name: 'Rogue',
        initiative: 18,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
      {
        id: 'c3',
        name: 'Cleric',
        initiative: 8,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    const sorted = sortCharactersByInitiative(chars);
    expect(sorted.map((c) => c.name)).toEqual(['Rogue', 'Fighter', 'Cleric']);
    expect(sorted.map((c) => c.initiative)).toEqual([18, 12, 8]);
  });

  // spec: alhalte-argenterie/field 2個1行ステータス表示・プログレスバー
  it('chunks statuses in pairs of 2 per row and formats progress bar values', () => {
    const statuses = [
      { label: 'HP', current: 8, max: 10 },
      { label: 'MP', current: 15, max: 20 },
      { label: 'SAN', current: 50, max: 80 },
    ];

    const pairs = chunkStatusesInPairs(statuses);
    expect(pairs).toHaveLength(2);
    expect(pairs[0]).toHaveLength(2);
    expect(pairs[1]).toHaveLength(1);

    const hpFormatted = formatStatusItem(statuses[0]);
    expect(hpFormatted.text).toBe('HP 10/8'); // ラベル名 最大値/現在地
    expect(hpFormatted.percentage).toBe(80); // (8 / 10) * 100%
  });

  // spec: alhalte-argenterie/field FieldPanel コンポーネント独立状態
  it('renders FieldPanel with top-left overlay and independently maintained view transform', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-field',
      name: '遭遇戦マップ',
      juices: createDefaultJuices(),
      viewTransform: { x: 50, y: 30, scale: 1.2 },
      members: ['c1', 'c2'],
    };

    const characters: Character[] = [
      {
        id: 'c1',
        name: 'アリス',
        initiative: 15,
        description: '探索者',
        statuses: [
          { label: 'HP', current: 12, max: 12 },
          { label: 'MP', current: 8, max: 10 },
        ],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
        placedAsPiece: true,
        piecePosition: { x: 120, y: 150 },
      },
      {
        id: 'c2',
        name: 'ボブ',
        initiative: 20,
        description: '用心棒',
        statuses: [{ label: 'HP', current: 5, max: 15 }],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    let recordedTransform: ViewTransform | null = null;
    const instance = mount(FieldPanel, {
      target,
      props: {
        roomName: 'テストルーム',
        scene: testScene,
        characters,
        onTransformChange: (scId, t) => {
          recordedTransform = t;
        },
      },
    });

    // Check room name badge
    expect(target.querySelector('[data-testid="room-name-badge"]')?.textContent?.trim()).toBe('テストルーム');

    // Check initiative sorting: Bob (20) before Alice (15)
    const charEntries = target.querySelectorAll('[data-testid^="char-entry-"]');
    expect(charEntries).toHaveLength(2);
    expect(charEntries[0].getAttribute('data-testid')).toBe('char-entry-c2');
    expect(charEntries[1].getAttribute('data-testid')).toBe('char-entry-c1');

    // Check initiative badges
    expect(target.querySelector('[data-testid="initiative-badge-c2"]')?.textContent?.trim()).toBe('20');
    expect(target.querySelector('[data-testid="initiative-badge-c1"]')?.textContent?.trim()).toBe('15');

    // Check status display
    const hpItem = target.querySelector('[data-testid="status-item-c1-HP"]');
    expect(hpItem?.textContent).toContain('HP 12/12');

    // Check piece placed on canvas
    expect(target.querySelector('[data-testid="piece-c1"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});
