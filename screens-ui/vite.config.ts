import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { resolve } from 'path';

export default defineConfig({
  plugins: [
    svelte(),
    viteSingleFile(),
  ],
  build: {
    outDir: resolve(__dirname, '../screens'),
    emptyOutDir: false,
    rollupOptions: {
      input: {
        dual: resolve(__dirname, 'dual.html'),
        left: resolve(__dirname, 'left.html'),
        right: resolve(__dirname, 'right.html'),
      },
    },
  },
});
