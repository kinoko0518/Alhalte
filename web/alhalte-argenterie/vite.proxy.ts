/**
 * Vite dev-server proxy map for `alhalte-argenterie`.
 *
 * Kept in its own module (imported by `vite.config.ts`) so tests can assert the
 * routing without evaluating the Vite config, which resolves `__dirname`,
 * plugins, and the library build entry.
 *
 * Vite treats a `server.proxy` key starting with `^` as a regular expression and
 * every other key as a plain URL prefix. maitre-d's room endpoints therefore use
 * the precise regex `^/room/` instead of the prefix `/room`: a prefix matches
 * the SPA route `/rooms/<roomId>` too, which made a direct room URL return
 * maitre-d's 404 instead of the SPA shell (Issue #224).
 *
 * Reference: `web/alhalte-argenterie/ARCHITECTURE.md` (`### HTTP` -> Vite dev proxy).
 */
import type { ProxyOptions } from 'vite';

/** `alhalte-maitre-d` origin used by the dev server. */
export const DEV_SERVER_ORIGIN = 'http://127.0.0.1:8080';

/** `alhalte-maitre-d` WebSocket origin used by the dev server. */
export const DEV_WS_SERVER_ORIGIN = 'ws://127.0.0.1:8080';

/**
 * Vite `server.proxy` map.
 *
 * - `/api`: room registry, session, cellar proxy, account endpoints.
 * - `/auth`: Google OAuth entry / callback; maitre-d's `--public-url` points at
 *   the Vite origin so the session cookie lands on the same origin as the UI.
 * - `^/room/`: maitre-d's non-`/api` room endpoints (`/room/{id}` WebSocket
 *   upgrade, `/room/{id}/ws`, `/room/{id}/export/zip`, `/room/{id}/permissions`).
 *   The regex keeps the client-side SPA route `/rooms/{id}` out of the proxy so
 *   it falls through to Vite's HTML fallback (`appType: 'spa'`).
 * - `/ws`: room WebSocket (`/ws/room/{id}`).
 */
export const viteProxy: Record<string, ProxyOptions> = {
  '/api': {
    target: DEV_SERVER_ORIGIN,
    changeOrigin: true,
  },
  // Google OAuth entry / callback (`/auth/login`, `/auth/google/callback`).
  // maitre-d's --public-url points at this dev origin so the session cookie
  // is set on the same origin the UI and /api/session use.
  '/auth': {
    target: DEV_SERVER_ORIGIN,
    changeOrigin: true,
  },
  // Regex, not a prefix: `/rooms/<roomId>` is an SPA route, `/room/<roomId>` is
  // maitre-d. Keep this key anchored so only `/room/...` is proxied (#224).
  '^/room/': {
    target: DEV_SERVER_ORIGIN,
    changeOrigin: true,
    ws: true,
  },
  '/ws': {
    target: DEV_WS_SERVER_ORIGIN,
    ws: true,
  },
};
