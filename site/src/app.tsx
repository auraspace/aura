import { Routes, Route } from 'react-router-dom'
import { Layout } from './components/layout'
import { HomePage } from './pages/home-page'
import { RfcPage } from './pages/rfc-page'
import { GraphPage } from './pages/graph-page'
import { NotFoundPage } from './pages/not-found-page'

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        {/* Catalog is canonical at /rfc (GitHub Pages: …/aura/rfc) */}
        <Route index element={<HomePage />} />
        <Route path="rfc" element={<HomePage />} />
        <Route path="rfc/:id" element={<RfcPage />} />
        <Route path="graph" element={<GraphPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  )
}

export default App
