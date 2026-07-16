import type { RfcStatus } from '@/lib/rfc/types'

const base =
  'inline-block rounded-full border border-border bg-card px-2 py-0.5 text-xs text-muted'

function statusClass(status: RfcStatus): string {
  switch (status) {
    case 'In Review':
      return `${base} border-status-review-border text-status-review`
    case 'Accepted':
    case 'Frozen':
      return `${base} border-status-ok-border text-status-ok`
    default:
      return base
  }
}

export function StatusBadge({ status }: { status: RfcStatus }) {
  return <span className={statusClass(status)}>{status}</span>
}
