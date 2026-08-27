import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath, URL } from 'node:url';

// Porta diversa da quella del launcher: i due `tauri dev` possono girare
// insieme, ed è comodo quando si prova un'installazione contro una build
// locale del launcher.
const DEV_PORT = 5174;

export default defineConfig({
  root: 'setup',
  // La radice di Vite è `setup/`, dove non c'è nessun `svelte.config.js`: va
  // indicato quello del progetto, altrimenti si perdono il preprocessore
  // TypeScript e la modalità runes.
  plugins: [svelte({ configFile: fileURLToPath(new URL('./svelte.config.js', import.meta.url)) })],
  resolve: {
    alias: {
      // L'installer usa gli stessi token, gli stessi stili e le stesse icone
      // del launcher: è la stessa applicazione, vista un attimo prima.
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
      $setup: fileURLToPath(new URL('./setup/src', import.meta.url))
    }
  },
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'esnext',
    sourcemap: true,
    chunkSizeWarningLimit: 1500
  }
});
