import { build } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

async function run() {
  for (const target of ['dual', 'left', 'right']) {
    console.log(`Building ${target}.html...`);
    await build({
      plugins: [svelte(), viteSingleFile()],
      build: {
        outDir: resolve(__dirname, '../screens'),
        emptyOutDir: false,
        rollupOptions: {
          input: resolve(__dirname, `${target}.html`),
        },
      },
      configFile: false,
    });
  }
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});
