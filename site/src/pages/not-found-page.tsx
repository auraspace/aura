import { Link } from 'react-router-dom'

export function NotFoundPage() {
  return (
    <main className="page-shell">
      <p className="eyebrow">404</p>
      <h1 className="mt-3 font-display text-[34px] font-medium tracking-tight">
        Not found
      </h1>
      <p className="mt-2 text-muted">That page does not exist.</p>
      <p className="mt-6">
        <Link to="/" className="btn-ghost">
          Back home
          <span aria-hidden="true">→</span>
        </Link>
      </p>
    </main>
  )
}
