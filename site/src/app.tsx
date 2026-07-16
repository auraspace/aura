import { Navigate, Route, Routes } from 'react-router-dom'
import { Layout } from '@/components/layout'
import { CatalogPage, DetailPage, GraphPage } from '@/pages/rfc'
import { NotFoundPage } from '@/pages/not-found-page'

/**
 * Route map (extensible for /docs, landing, …):
 *   /              → redirect to /rfc
 *   /rfc           → catalog
 *   /rfc/:id       → detail
 *   /rfc/graph     → dependency graph
 *   *              → 404
 */
export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Navigate to="/rfc" replace />} />
        <Route path="rfc">
          <Route index element={<CatalogPage />} />
          <Route path="graph" element={<GraphPage />} />
          <Route path=":id" element={<DetailPage />} />
        </Route>
        {/* Legacy graph URL */}
        <Route path="graph" element={<Navigate to="/rfc/graph" replace />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  )
}

export default App
