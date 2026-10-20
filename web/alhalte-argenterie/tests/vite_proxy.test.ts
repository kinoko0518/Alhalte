/**
 * Regression tests for the Vite dev-server proxy map (Issue #224).
 *
 * A direct visit to the SPA route `/rooms/<roomId>#<roomKey>` must be served by
 * Vite's HTML fallback, not proxied to `alhalte-maitre-d` (which has no
 * `/rooms/...` route and answers 404). maitre-d's room endpoints live under
 * `/room/...` and must stay proxied.
 *
 * Vite's `server.proxy` semantics (vite 8 `createProxyContextMatcher`): a key
 * starting with `^` is a regular expression tested against the request URL;
 * every other key is a plain prefix match. The matcher below mirrors that so the
 * test never starts a dev server.
 *
 * Reference: `web/alhalte-argenterie/ARCHITECTURE.md` (`### HTTP` -> Vite dev proxy).
 */
import { describe, it, expect } from 'vitest';
import {
  DEV_SERVER_ORIGIN,
  DEV_WS_SERVER_ORIGIN,
  viteProxy,
} from '../vite.proxy.js';

/** Mirrors Vite 8's `createProxyContextMatcher` for a single proxy key. */
function matchesProxyContext(context: string, url: string): boolean {
  if (context.startsWith('^')) {
    return new RegExp(context).test(url);
  }
  return url.startsWith(context);
}

/** Target of the first proxy key matching `url`, or `null` when Vite serves it. */
function proxyTargetFor(url: string): string | null {
  for (const [context, options] of Object.entries(viteProxy)) {
    if (!matchesProxyContext(context, url)) continue;
    return typeof options === 'string' ? options : options.target ?? null;
  }
  return null;
}

function proxyOptionsFor(url: string) {
  for (const [context, options] of Object.entries(viteProxy)) {
    if (!matchesProxyContext(context, url)) continue;
    return options;
  }
  return null;
}

describe('Vite dev proxy routing (Issue #224)', () => {
  it('keeps the client-side SPA route /rooms/<id> out of every proxy key', () => {
    const spaPaths = [
      '/rooms/0102030405060708',
      '/rooms/0102030405060708/',
      '/rooms/f39403e73f32ec91?foo=bar',
      '/rooms/f39403e73f32ec91/',
    ];

    for (const path of spaPaths) {
      // No proxy key may match: Vite then serves index.html and App resolves the
      // room from window.location (fragment key included).
      for (const context of Object.keys(viteProxy)) {
        expect(
          matchesProxyContext(context, path),
          `proxy key ${context} unexpectedly matches SPA path ${path}`,
        ).toBe(false);
      }
      expect(proxyTargetFor(path)).toBeNull();
    }

    // The old broken key was the bare prefix `/room`, which also matched
    // `/rooms/...`; assert it is gone.
    expect(Object.keys(viteProxy)).not.toContain('/room');
    // A lookalike path must not be proxied either (regex, not prefix).
    expect(proxyTargetFor('/roommate')).toBeNull();
  });

  it('still proxies maitre-d room endpoints under /room/<id>', () => {
    const roomPaths = [
      '/room/0102030405060708',
      '/room/f39403e73f32ec91/ws',
      '/room/f39403e73f32ec91/export/zip?type=full',
      '/room/f39403e73f32ec91/permissions',
    ];

    for (const path of roomPaths) {
      expect(matchesProxyContext('^/room/', path)).toBe(true);
      expect(proxyTargetFor(path)).toBe(DEV_SERVER_ORIGIN);
    }

    // WebSocket upgrades for `/room/{id}` need `ws: true`.
    expect(proxyOptionsFor('/room/0102030405060708')).toMatchObject({
      ws: true,
    });
  });

  it('keeps the /api, /auth, and /ws proxies working', () => {
    expect(proxyTargetFor('/api/rooms/0102030405060708')).toBe(DEV_SERVER_ORIGIN);
    expect(proxyTargetFor('/api/cellar/store')).toBe(DEV_SERVER_ORIGIN);
    expect(proxyTargetFor('/auth/login?return_to=%2Frooms%2F0102030405060708')).toBe(
      DEV_SERVER_ORIGIN,
    );
    expect(proxyTargetFor('/auth/google/callback?code=x')).toBe(DEV_SERVER_ORIGIN);
    expect(proxyTargetFor('/ws/room/0102030405060708?account_id=1')).toBe(
      DEV_WS_SERVER_ORIGIN,
    );
    expect(proxyOptionsFor('/ws/room/0102030405060708')).toMatchObject({
      ws: true,
    });
  });
});
