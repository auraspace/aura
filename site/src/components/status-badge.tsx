import type { RfcStatus } from '@/types/rfc'

function statusClass(status: RfcStatus): string {
  return `badge status-${status.toLowerCase().replace(/\s+/g, '-')}`
}

export function StatusBadge({ status }: { status: RfcStatus }) {
  return <span className={statusClass(status)}>{status}</span>
}
