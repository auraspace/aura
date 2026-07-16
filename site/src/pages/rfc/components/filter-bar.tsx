import type { RfcStatus } from '@/lib/rfc/types'

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

const fieldClass =
  'min-w-40 rounded-[0.4rem] border border-border bg-card px-2.5 py-1.5 text-fg'

export function FilterBar({
  status,
  layer,
  layers,
  onStatusChange,
  onLayerChange,
}: FilterBarProps) {
  return (
    <>
      <label className="flex flex-col gap-1 text-xs text-muted">
        Status
        <select
          className={fieldClass}
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
      <label className="flex flex-col gap-1 text-xs text-muted">
        Layer
        <select
          className={fieldClass}
          value={layer}
          onChange={(e) => onLayerChange(e.target.value)}
        >
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
