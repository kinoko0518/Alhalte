/**
 * Tests for the offline queue notice component (Issue #109 / sub-issue #137).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - GitHub Issue #109: `オフライン動作`
 * - GitHub Issue #137: `オフラインキュー通知 UI`
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import OfflineQueueNotice from '../src/components/OfflineQueueNotice.svelte';

type NoticeKind = 'message' | 'character' | 'generic';

interface NoticeProps {
  kind?: NoticeKind;
  description?: string;
  count?: number;
  class?: string;
}

const SOURCE = readFileSync(
  path.resolve(process.cwd(), 'src/components/OfflineQueueNotice.svelte'),
  'utf8',
);

describe('OfflineQueueNotice (Issue #137)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  function render(props: NoticeProps = {}) {
    return mount(OfflineQueueNotice, { target, props });
  }

  function notice(): HTMLElement {
    const element = target.querySelector<HTMLElement>('[data-testid="offline-queue-notice"]');
    expect(element).toBeTruthy();
    return element!;
  }

  // spec: alhalte-argenterie/offline-queue-notice 波線区切りの中に「サーバーと通信できませんでした」
  it('renders the canonical title inside wavy dividers', () => {
    const instance = render({ kind: 'message' });

    const element = notice();
    expect(element.querySelector('[data-testid="offline-queue-notice-title"]')?.textContent?.trim()).toBe(
      'サーバーと通信できませんでした',
    );

    const dividers = element.querySelectorAll<HTMLElement>('.wavy-divider');
    expect(dividers).toHaveLength(2);
    expect(dividers[0]?.getAttribute('data-testid')).toBe('offline-queue-notice-divider');
    for (const divider of dividers) {
      expect(divider.getAttribute('aria-hidden')).toBe('true');
    }

    unmount(instance);
  });

  // spec: alhalte-argenterie/offline-queue-notice kind ごとの説明文
  it('shows the message description for kind="message"', () => {
    const instance = render({ kind: 'message' });

    expect(notice().getAttribute('data-kind')).toBe('message');
    expect(
      target.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent?.trim(),
    ).toBe('メッセージは次回の接続時に送信されます');
    expect(target.querySelector('[data-testid="offline-queue-notice-count"]')).toBeNull();

    unmount(instance);
  });

  it('shows the character description for kind="character"', () => {
    const instance = render({ kind: 'character' });

    expect(notice().getAttribute('data-kind')).toBe('character');
    expect(
      target.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent?.trim(),
    ).toBe('キャラクターは次回の接続時に更新されます');

    unmount(instance);
  });

  it('uses the description prop for kind="generic"', () => {
    const instance = render({ kind: 'generic', description: 'シーンは次回の接続時に同期されます' });

    expect(notice().getAttribute('data-kind')).toBe('generic');
    expect(
      target.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent?.trim(),
    ).toBe('シーンは次回の接続時に同期されます');

    unmount(instance);
  });

  it('defaults to the generic description when none is provided', () => {
    const instance = render();

    expect(notice().getAttribute('data-kind')).toBe('generic');
    expect(
      target.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent?.trim(),
    ).not.toBe('');

    unmount(instance);
  });

  // spec: alhalte-argenterie/offline-queue-notice count を渡すと「N 件を蓄積中」
  it('renders the queued count when provided, including zero', () => {
    const three = render({ kind: 'message', count: 3 });
    expect(target.querySelector('[data-testid="offline-queue-notice-count"]')?.textContent?.trim()).toBe(
      '3 件を蓄積中',
    );
    unmount(three);

    const zero = render({ kind: 'message', count: 0 });
    expect(target.querySelector('[data-testid="offline-queue-notice-count"]')?.textContent?.trim()).toBe(
      '0 件を蓄積中',
    );
    unmount(zero);
  });

  // spec: alhalte-argenterie/offline-queue-notice role=status + aria-live=polite
  it('is a polite live region that does not duplicate the connection bar', () => {
    const instance = render({ kind: 'message', count: 2 });

    const element = notice();
    expect(element.getAttribute('role')).toBe('status');
    expect(element.getAttribute('aria-live')).toBe('polite');
    expect(element.getAttribute('aria-atomic')).toBe('true');
    // The global connection bar was removed in Issue #261; the queue notice
    // must stay local to the log instead of recreating it.
    expect(element.getAttribute('data-testid')).not.toBe('offline-indicator');
    expect(element.textContent).not.toContain('サーバーとの接続が切断されました');
    expect(element.textContent).not.toContain('再接続');

    unmount(instance);
  });

  it('applies an optional parent class alongside its own class', () => {
    const instance = render({ kind: 'message', class: 'queue-slot' });

    expect(notice().classList.contains('offline-queue-notice')).toBe(true);
    expect(notice().classList.contains('queue-slot')).toBe(true);

    unmount(instance);
  });

  // spec: alhalte-argenterie/offline-queue-notice 画像に依存しない CSS 波線
  it('draws the wavy divider with CSS and no image assets', () => {
    const instance = render({ kind: 'message' });
    const dividers = target.querySelectorAll<HTMLElement>('.wavy-divider');
    expect(dividers).toHaveLength(2);
    for (const divider of dividers) {
      expect(divider.classList.contains('wavy-divider')).toBe(true);
      expect(divider.getAttribute('aria-hidden')).toBe('true');
    }

    // No <img>, <picture>, or <svg> content: the wave is pure CSS.
    expect(target.querySelector('img')).toBeNull();
    expect(target.querySelector('picture')).toBeNull();
    expect(target.querySelector('svg')).toBeNull();

    // jsdom does not apply Svelte's scoped component styles, so the gradient
    // itself is asserted against the component stylesheet below.
    expect(SOURCE).toMatch(/\.wavy-divider\s*\{[^}]*radial-gradient/);

    unmount(instance);
  });

  it('keeps the component styles on semantic M3 tokens with no hard-coded colors', () => {
    const style = SOURCE.match(/<style>([\s\S]*?)<\/style>/)?.[1] ?? '';
    expect(style).not.toBe('');

    // Wavy divider must be gradient based, never an image URL.
    expect(style).toMatch(/(?:repeating-)?radial-gradient\(/);
    expect(style).not.toMatch(/url\(/);
    expect(SOURCE).not.toMatch(/<img\b/);

    // Colors come from noph-ui semantic tokens only.
    expect(style).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(style).not.toMatch(/\b(?:rgb|rgba|hsl|hsla|oklch|oklab|lab|lch)\(/);
    expect(style).toMatch(/var\(--np-color-/);
  });
});
