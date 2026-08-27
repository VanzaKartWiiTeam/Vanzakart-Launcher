import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath, URL } from 'node:url';

// La porta è fissa: `tauri.conf.json` ci punta con `devUrl`.
const DEV_PORT = 5173;

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url))
    }
  },
  // Tauri gestisce l'apertura della finestra: il browser non va aperto.
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    watch: {
      // La ricompilazione di Rust non deve far ripartire Vite.
      ignored: ['**/src-tauri/**']
    }
  },
  build: {
    target: 'esnext',
    sourcemap: true,
    // Il bundle finisce dentro una webview locale: niente code splitting
    // aggressivo, un solo chunk si carica più in fretta.
    chunkSizeWarningLimit: 1500
  }
});
