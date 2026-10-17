import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import FieldPiece from '../src/components/Field/FieldPiece.svelte';
import FieldPanel from '../src/components/Field/FieldPanel.svelte';
import type { Character, Scene } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

describe('Field Piece Drag & Ghost Suppression', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
  });

  // spec: alhalte-argenterie/field/piece 駒画像 draggable="false" とゴースト抑制
  it('ensures piece image has draggable="false" and suppresses native browser drag-and-drop', () => {
    const character: Character = {
      id: 'char-1',
      name: '探索者アルト',
      initiative: 10,
      iconUrl: 'https://example.com/avatar.png',
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      placedAsPiece: true,
      piecePosition: { x: 150, y: 200 },
    };

    const instance = mount(FieldPiece, {
      target,
      props: {
        character,
        scale: 1.0,
      },
    });

    const pieceEl = target.querySelector('[data-testid="piece-char-1"]') as HTMLElement;
    expect(pieceEl).toBeTruthy();
    expect(pieceEl.getAttribute('draggable')).toBe('false');

    const imgEl = pieceEl.querySelector('img.piece-avatar') as HTMLImageElement;
    expect(imgEl).toBeTruthy();
    expect(imgEl.getAttribute('draggable')).toBe('false');

    // Verify ondragstart prevents default to suppress native browser ghost drag
    const dragStartEvent = new DragEvent('dragstart', {
      bubbles: true,
      cancelable: true,
    });
    const dispatched = imgEl.dispatchEvent(dragStartEvent);
    expect(dispatched).toBe(false);
    expect(dragStartEvent.defaultPrevented).toBe(true);

    const pieceDragEvent = new DragEvent('dragstart', {
      bubbles: true,
      cancelable: true,
    });
    const pieceDispatched = pieceEl.dispatchEvent(pieceDragEvent);
    expect(pieceDispatched).toBe(false);
    expect(pieceDragEvent.defaultPrevented).toBe(true);

    unmount(instance);
  });

  // spec: alhalte-argenterie/field/piece Pointer Events によるドラッグ操作とポインターキャプチャ
  it('updates piece position using Pointer Events and captures/releases pointer', () => {
    const character: Character = {
      id: 'char-2',
      name: '戦士ボブ',
      initiative: 15,
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      placedAsPiece: true,
      piecePosition: { x: 100, y: 100 },
    };

    let movedPos: { x: number; y: number } | null = null;
    let dragStarted = false;
    let dragEnded = false;

    const instance = mount(FieldPiece, {
      target,
      props: {
        character,
        scale: 1.0,
        onMove: (pos) => {
          movedPos = pos;
        },
        onDragStart: () => {
          dragStarted = true;
        },
        onDragEnd: () => {
          dragEnded = true;
        },
      },
    });

    const pieceEl = target.querySelector('[data-testid="piece-char-2"]') as HTMLElement;
    expect(pieceEl).toBeTruthy();

    const setCaptureSpy = vi.spyOn(pieceEl, 'setPointerCapture');
    const releaseCaptureSpy = vi.spyOn(pieceEl, 'releasePointerCapture');

    // 1. pointerdown at (50, 50)
    const downEvent = new PointerEvent('pointerdown', {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId: 42,
      clientX: 50,
      clientY: 50,
    });
    pieceEl.dispatchEvent(downEvent);

    expect(dragStarted).toBe(true);
    expect(setCaptureSpy).toHaveBeenCalledWith(42);

    // 2. pointermove to (80, 70) -> dx = +30, dy = +20
    const moveEvent = new PointerEvent('pointermove', {
      bubbles: true,
      cancelable: true,
      pointerId: 42,
      clientX: 80,
      clientY: 70,
    });
    pieceEl.dispatchEvent(moveEvent);

    // 3. pointerup at (80, 70)
    const upEvent = new PointerEvent('pointerup', {
      bubbles: true,
      cancelable: true,
      pointerId: 42,
      clientX: 80,
      clientY: 70,
    });
    pieceEl.dispatchEvent(upEvent);

    expect(dragEnded).toBe(true);
    expect(releaseCaptureSpy).toHaveBeenCalledWith(42);
    expect(movedPos).toEqual({ x: 130, y: 120 });
    expect(character.piecePosition).toEqual({ x: 130, y: 120 });

    unmount(instance);
  });

  // spec: alhalte-argenterie/field/piece ズーム（scale）時の移動差分適用 ((clientX - startX) / scale)
  it('applies scale compensation to pointer movement delta', () => {
    // Test scale = 2.0 (zoomed in: physical delta 60px should become canvas delta 30px)
    const charZoomed: Character = {
      id: 'char-zoom-2',
      name: '拡大駒',
      initiative: 10,
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      placedAsPiece: true,
      piecePosition: { x: 100, y: 100 },
    };

    let posZoomed: { x: number; y: number } | null = null;
    const instance2x = mount(FieldPiece, {
      target,
      props: {
        character: charZoomed,
        scale: 2.0,
        onMove: (pos) => {
          posZoomed = pos;
        },
      },
    });

    const piece2x = target.querySelector('[data-testid="piece-char-zoom-2"]') as HTMLElement;
    piece2x.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        button: 0,
        pointerId: 1,
        clientX: 100,
        clientY: 100,
      })
    );
    piece2x.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 1,
        clientX: 160, // dx = 60 -> 60 / 2.0 = 30
        clientY: 140, // dy = 40 -> 40 / 2.0 = 20
      })
    );
    piece2x.dispatchEvent(
      new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId: 1,
        clientX: 160,
        clientY: 140,
      })
    );

    expect(posZoomed).toEqual({ x: 130, y: 120 });
    unmount(instance2x);

    // Test scale = 0.5 (zoomed out: physical delta 60px should become canvas delta 120px)
    const charShrunk: Character = {
      id: 'char-zoom-half',
      name: '縮小駒',
      initiative: 10,
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      placedAsPiece: true,
      piecePosition: { x: 100, y: 100 },
    };

    let posShrunk: { x: number; y: number } | null = null;
    const instanceHalf = mount(FieldPiece, {
      target,
      props: {
        character: charShrunk,
        scale: 0.5,
        onMove: (pos) => {
          posShrunk = pos;
        },
      },
    });

    const pieceHalf = target.querySelector('[data-testid="piece-char-zoom-half"]') as HTMLElement;
    pieceHalf.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        button: 0,
        pointerId: 2,
        clientX: 100,
        clientY: 100,
      })
    );
    pieceHalf.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 2,
        clientX: 160, // dx = 60 -> 60 / 0.5 = 120
        clientY: 140, // dy = 40 -> 40 / 0.5 = 80
      })
    );
    pieceHalf.dispatchEvent(
      new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId: 2,
        clientX: 160,
        clientY: 140,
      })
    );

    expect(posShrunk).toEqual({ x: 220, y: 180 });
    unmount(instanceHalf);
  });

  // spec: alhalte-argenterie/field/piece pointercancel 時のポインターキャプチャ解放と位置確定
  it('handles pointercancel cleanly by releasing capture and committing position', async () => {
    const character: Character = {
      id: 'char-cancel',
      name: 'キャンセル駒',
      initiative: 10,
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      placedAsPiece: true,
      piecePosition: { x: 50, y: 50 },
    };

    let committedPos: { x: number; y: number } | null = null;
    const instance = mount(FieldPiece, {
      target,
      props: {
        character,
        scale: 1.0,
        onMove: (pos) => {
          committedPos = pos;
        },
      },
    });

    // Flush microtasks so Svelte attaches non-delegated pointer events
    await Promise.resolve();

    const pieceEl = target.querySelector('[data-testid="piece-char-cancel"]') as HTMLElement;
    const releaseCaptureSpy = vi.spyOn(pieceEl, 'releasePointerCapture');

    pieceEl.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        button: 0,
        pointerId: 99,
        clientX: 10,
        clientY: 10,
      })
    );
    pieceEl.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 99,
        clientX: 40,
        clientY: 30,
      })
    );
    pieceEl.dispatchEvent(
      new PointerEvent('pointercancel', {
        bubbles: true,
        cancelable: true,
        pointerId: 99,
        clientX: 40,
        clientY: 30,
      })
    );

    expect(releaseCaptureSpy).toHaveBeenCalledWith(99);
    expect(committedPos).toEqual({ x: 80, y: 70 });

    unmount(instance);
  });

  // spec: alhalte-argenterie/field/piece FieldPanel での駒ドラッグとキャンバスパンの干渉防止
  it('prevents event propagation so piece dragging does not cause canvas pan in FieldPanel', () => {
    const testScene: Scene = {
      id: 'scene-drag-test',
      name: 'ドラッグテスト',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1.0 },
      members: ['c-hero'],
    };

    const characters: Character[] = [
      {
        id: 'c-hero',
        name: '主人公',
        initiative: 20,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
        placedAsPiece: true,
        piecePosition: { x: 100, y: 100 },
      },
    ];

    let transformChanged = false;
    let pieceMovedPos: { x: number; y: number } | null = null;

    const instance = mount(FieldPanel, {
      target,
      props: {
        scene: testScene,
        characters,
        onTransformChange: () => {
          transformChanged = true;
        },
        onPieceMove: (charId, pos) => {
          if (charId === 'c-hero') pieceMovedPos = pos;
        },
      },
    });

    const pieceEl = target.querySelector('[data-testid="piece-c-hero"]') as HTMLElement;
    expect(pieceEl).toBeTruthy();

    // Drag piece on board
    pieceEl.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        button: 0,
        pointerId: 10,
        clientX: 100,
        clientY: 100,
      })
    );
    pieceEl.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 10,
        clientX: 150,
        clientY: 120,
      })
    );
    pieceEl.dispatchEvent(
      new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId: 10,
        clientX: 150,
        clientY: 120,
      })
    );

    // Piece position should be updated
    expect(pieceMovedPos).toEqual({ x: 150, y: 120 });
    // Canvas view transform should NOT change because propagation was stopped
    expect(transformChanged).toBe(false);

    unmount(instance);
  });
});
