import { defineConfig, mergeConfig } from 'vitest/config';
import { fileURLToPath, URL } from 'node:url';

import viteConfig from './vite.config.ts';

export default mergeConfig(
  viteConfig,
  defineConfig({
    // L'alias `$setup` non serve al launcher, ma i test dell'installer
    // stanno nello stesso progetto e lo usano.
    resolve: {
      alias: {
        $setup: fileURLToPath(new URL('./setup/src', import.meta.url))
      }
    },
    test: {
      environment: 'jsdom',
      include: ['src/**/*.test.ts', 'setup/src/**/*.test.ts'],
      globals: true
    }
  })
);
