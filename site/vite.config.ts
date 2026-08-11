import path from 'node:path'

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const base = process.env.VITE_BASE || '/'

export default defineConfig({
  base,
  plugins: [react(), tailwindcss()],
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
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('/node_modules/@tabler/icons-react/')) {
            return 'icons'
          }
          if (id.includes('/node_modules/@shikijs/core/')) return 'syntax-core'
          if (id.includes('/node_modules/@shikijs/engine-javascript/'))
            return 'syntax-engine'
          if (id.includes('/node_modules/react/')) return 'react'
          if (id.includes('/node_modules/react-dom/')) return 'react-dom'
          if (id.includes('/node_modules/react-router')) return 'router'
          if (id.includes('/node_modules/react-markdown/'))
            return 'markdown-runtime'
          if (id.includes('/node_modules/motion/')) return 'motion'
          if (id.includes('/node_modules/minisearch/')) return 'search'
          if (id.includes('/node_modules/github-slugger/')) return 'slugs'
          if (id.includes('/node_modules/remark-')) return 'markdown-plugins'
          if (id.includes('/node_modules/rehype-')) return 'markdown-plugins'
          if (id.includes('/site/src/pages/home-page')) return 'page-home'
          if (id.includes('/site/src/pages/docs/')) return 'page-docs'
          if (id.includes('/site/src/pages/rfc/')) return 'page-rfc'
        },
      },
    },
  },
})
