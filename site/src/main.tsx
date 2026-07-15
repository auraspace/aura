import { StrictMode } from 'react'
import { createRoot, hydrateRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { App } from './app'
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

const el = document.getElementById('root')!
if (el.hasChildNodes()) {
  hydrateRoot(el, tree)
} else {
  createRoot(el).render(tree)
}
