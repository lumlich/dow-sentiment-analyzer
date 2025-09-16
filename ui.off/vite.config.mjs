// ui.off/vite.config.mjs
import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC_PATH = path.resolve(__dirname, 'src');

export default defineConfig({
  plugins: [preact()],
  resolve: {
    alias: { '@': SRC_PATH },
  },
  server: {
    proxy: {
      '/decide':  { target: 'http://127.0.0.1:8000', changeOrigin: true },
      '/analyze': { target: 'http://127.0.0.1:8000', changeOrigin: true },
      '/health':  { target: 'http://127.0.0.1:8000', changeOrigin: true },
    },
  },
});
