/// <reference types="vitest/config" />
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: { proxy: { '/api': { target: 'http://127.0.0.1:8090', changeOrigin: true } } },
  test: { environment: 'jsdom', css: true, setupFiles: ['./src/test/setup.ts'] },
})
