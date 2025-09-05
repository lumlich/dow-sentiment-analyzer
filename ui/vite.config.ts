import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';

// ESM-safe absolute path to ./src bez __dirname a bez node:path
const SRC_PATH = new URL('./src', import.meta.url).pathname;

// Dev proxy: UI -> Shuttle backend on 127.0.0.1:8000
export default defineConfig({
  plugins: [preact()],
  resolve: {
    alias: {
      '@': SRC_PATH,
    },
  },
  server: {
    proxy: {
      '/decide': {
        target: 'http://127.0.0.1:8000',
        changeOrigin: true,
      },
      '/analyze': {
        target: 'http://127.0.0.1:8000',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://127.0.0.1:8000',
        changeOrigin: true,
      },
    },
  },
});
