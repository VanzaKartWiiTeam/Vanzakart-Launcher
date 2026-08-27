import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
  compilerOptions: {
    // Svelte 5 in modalità runes: niente store impliciti, reattività esplicita.
    runes: true
  }
};
