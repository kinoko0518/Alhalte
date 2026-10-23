import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

/**
 * Production SPA build for `alhalte-maitre-d --web-root`.
 *
 * The normal `vite.config.ts` is a library build (it externalizes Svelte and
 * noph-ui and does not emit `index.html`). This configuration bundles the
 * application, including the `/login` route, and copies `public/` (extension
 * wasm, saignant wasm) into `dist-app/`.
 */
export default defineConfig({
  plugins: [svelte()],
  appType: 'spa',
  resolve: {
    conditions: ['svelte', 'browser'],
    alias: {
      '@alhalte/pantry': path.resolve(__dirname, '../alhalte-pantry/src/index.ts'),
    },
  },
  build: {
    outDir: 'dist-app',
    emptyOutDir: true,
  },
});
