import { Link } from 'react-router-dom'

import { getRfcById } from '@/lib/rfc/load-rfcs'

export function DepLinks({ label, ids }: { label: string; ids: string[] }) {
  if (!ids.length) {
    return (
      <p className="my-1.5 text-[0.9rem] text-muted">
        <strong>{label}:</strong> —
      </p>
    )
  }

  return (
    <p className="my-1.5 text-[0.9rem] text-muted">
      <strong>{label}:</strong>{' '}
      {ids.map((id) => {
        const doc = getRfcById(id)
        if (!doc) {
          return (
            <span key={id} className="text-danger line-through">
              RFC-{id}{' '}
            </span>
          )
        }
        return (
          <Link key={id} to={`/rfc/${id}`} className="mr-1.5">
            RFC-{id}
          </Link>
        )
      })}
    </p>
  )
}
