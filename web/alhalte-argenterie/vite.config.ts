import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';
import { viteProxy } from './vite.proxy.js';

export default defineConfig({
  plugins: [svelte()],
  // Serve the SPA shell for unknown paths (e.g. the direct room URL
  // `/rooms/<roomId>#<roomKey>`) instead of answering 404. This is Vite's
  // default; set explicitly to document why `/rooms/...` must not be proxied.
  appType: 'spa',
  server: {
    // `/rooms/<roomId>` is a client-side route and must fall through to the SPA
    // fallback above; only maitre-d's `/room/...` endpoints are proxied (#224).
    proxy: viteProxy,
  },
  resolve: {
    conditions: ['svelte', 'browser'],
    alias: {
      '@alhalte/pantry': path.resolve(__dirname, '../alhalte-pantry/src/index.ts'),
    },
  },
  build: {
    lib: {
      entry: path.resolve(__dirname, 'src/index.ts'),
      name: 'AlhalteArgenterie',
      fileName: 'index',
      formats: ['es'],
    },
    rollupOptions: {
      external: [
        'svelte',
        'svelte/internal',
        'noph-ui',
        '@alhalte/pantry',
      ],
    },
  },
});
