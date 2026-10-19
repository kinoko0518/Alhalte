import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
      // Google OAuth entry / callback (`/auth/login`, `/auth/google/callback`).
      // maitre-d's --public-url points at this dev origin so the session cookie
      // is set on the same origin the UI and /api/session use.
      '/auth': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
      '/room': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
        ws: true,
      },
      '/ws': {
        target: 'ws://127.0.0.1:8080',
        ws: true,
      },
    },
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
