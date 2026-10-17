import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';

describe('App Layout (Material 3 Expressive)', () => {
  const dummyRoom: VisitedRoom = {
    id: 'room-main',
    name: 'テスト部屋',
    url: 'https://alhalte.example/rooms/room-main#abcdef',
    lastVisited: Date.now(),
  };

  // spec: alhalte-argenterie/layout 横長画面（landscape）
  it('renders field panel on left and control panel on right in landscape mode', () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
      },
    });

    const roomLayout = target.querySelector('[data-testid="room-layout"]');
    expect(roomLayout?.classList.contains('landscape')).toBe(true);

    const fieldWrapper = target.querySelector('[data-testid="landscape-field-wrapper"]');
    const controlWrapper = target.querySelector('[data-testid="landscape-control-wrapper"]');

    expect(fieldWrapper).toBeTruthy();
    expect(controlWrapper).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/layout 縦長画面（portrait）
  it('renders full field with bottom sheet control panel in portrait mode', () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
      },
    });

    const roomLayout = target.querySelector('[data-testid="room-layout"]');
    expect(roomLayout?.classList.contains('portrait')).toBe(true);

    const portraitField = target.querySelector('[data-testid="portrait-field-wrapper"]');
    const bottomSheet = target.querySelector('[data-testid="portrait-bottom-sheet"]');

    expect(portraitField).toBeTruthy();
    expect(bottomSheet).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});
