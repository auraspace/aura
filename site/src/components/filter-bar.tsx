import type { RfcStatus } from '@/types/rfc'

const STATUSES: Array<RfcStatus | ''> = [
  '',
  'Draft',
  'In Review',
  'Accepted',
  'Frozen',
  'Rejected',
  'Superseded',
]

interface FilterBarProps {
  status: string
  layer: string
  layers: string[]
  onStatusChange: (v: string) => void
  onLayerChange: (v: string) => void
}

export function FilterBar({
  status,
  layer,
  layers,
  onStatusChange,
  onLayerChange,
}: FilterBarProps) {
  return (
    <>
      <label>
        Status
        <select
          value={status}
          onChange={(e) => onStatusChange(e.target.value)}
        >
          <option value="">All</option>
          {STATUSES.filter(Boolean).map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </label>
      <label>
        Layer
        <select value={layer} onChange={(e) => onLayerChange(e.target.value)}>
          <option value="">All</option>
          {layers.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
      </label>
    </>
  )
}
