import { StrictMode } from 'react'
import { createRoot, hydrateRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { App } from './app'
import { ensureHighlighter } from './lib/highlight'
import './styles/global.css'

const rawBase = import.meta.env.BASE_URL || '/'
const basename = rawBase === '/' ? undefined : rawBase.replace(/\/$/, '')

const tree = (
  <StrictMode>
    <BrowserRouter basename={basename}>
      <App />
    </BrowserRouter>
  </StrictMode>
)

async function boot() {
  await ensureHighlighter()
  const el = document.getElementById('root')!
  if (el.hasChildNodes()) {
    hydrateRoot(el, tree)
  } else {
    createRoot(el).render(tree)
  }
}

boot().catch((err) => {
  console.error(err)
  const el = document.getElementById('root')!
  createRoot(el).render(tree)
})
