import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig({
  plugins: [svelte()],
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
