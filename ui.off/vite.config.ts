// ui.off/vite.config.ts
import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';

// All code and comments intentionally in English.
export default defineConfig({
  plugins: [preact()],
  base: '/', // ensures absolute /assets/... URLs
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      // Dev only: calls to /api/* go to local Shuttle backend
      '/api': {
        target: 'http://127.0.0.1:8000',
        changeOrigin: true
      }
    }
  },
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    sourcemap: false
  }
});
