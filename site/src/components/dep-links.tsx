import { Link } from 'react-router-dom'
import { getRfcById } from '@/lib/load-rfcs'

export function DepLinks({
  label,
  ids,
}: {
  label: string
  ids: string[]
}) {
  if (!ids.length) {
    return (
      <p className="dep-links">
        <strong>{label}:</strong> —
      </p>
    )
  }

  return (
    <p className="dep-links">
      <strong>{label}:</strong>{' '}
      {ids.map((id) => {
        const doc = getRfcById(id)
        if (!doc) {
          return (
            <span key={id} className="missing">
              RFC-{id}{' '}
            </span>
          )
        }
        return (
          <Link key={id} to={`/rfc/${id}`}>
            RFC-{id}
          </Link>
        )
      })}
    </p>
  )
}
