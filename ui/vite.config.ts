import { defineConfig } from 'vite'
import preact from '@preact/preset-vite'

// Dev proxy: UI -> Shuttle backend on 127.0.0.1:8000
export default defineConfig({
  plugins: [preact()],
  server: {
    proxy: {
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
})