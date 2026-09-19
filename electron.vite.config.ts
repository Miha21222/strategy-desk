import { resolve } from 'node:path';
import { defineConfig, externalizeDepsPlugin } from 'electron-vite';
import preact from '@preact/preset-vite';

// Main and preload are bundled as CommonJS (Electron default); the Agent SDK
// is ESM-only and is loaded from the main process via dynamic import().
export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    resolve: { alias: { '@shared': resolve('src/shared') } },
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    resolve: { alias: { '@shared': resolve('src/shared') } },
  },
  renderer: {
    plugins: [preact()],
    resolve: {
      alias: { '@shared': resolve('src/shared'), '@renderer': resolve('src/renderer') },
    },
  },
});
