import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'

const base = process.env.VITE_BASE || '/'

export default defineConfig({
  base,
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
  server: {
    fs: {
      allow: [path.resolve(__dirname, '..')],
    },
  },
})
