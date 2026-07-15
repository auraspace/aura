import { Link } from 'react-router-dom'

export function NotFoundPage() {
  return (
    <div>
      <h1>Not found</h1>
      <p className="muted">That page does not exist.</p>
      <p>
        <Link to="/">Back to catalog</Link>
      </p>
    </div>
  )
}
