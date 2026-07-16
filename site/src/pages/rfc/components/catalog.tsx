import { Link } from 'react-router-dom'
import type { RfcMeta } from '@/lib/rfc/types'
import { StatusBadge } from './status-badge'
import { LayerChip } from './layer-chip'

interface RfcCatalogProps {
  items: RfcMeta[]
  /** When set, rows whose id is not in the set get data-hidden (client filter). */
  visibleIds?: Set<string> | null
}

export function RfcCatalog({ items, visibleIds }: RfcCatalogProps) {
  return (
    <table className="w-full overflow-hidden rounded-2xl border border-border border-collapse bg-card">
      <thead>
        <tr>
          <th className="bg-tint px-3 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.12em] text-muted uppercase align-top">
            RFC
          </th>
          <th className="bg-tint px-3 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.12em] text-muted uppercase align-top">
            Title
          </th>
          <th className="bg-tint px-3 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.12em] text-muted uppercase align-top">
            Status
          </th>
          <th className="bg-tint px-3 py-2.5 text-left font-mono text-[11px] font-medium tracking-[0.12em] text-muted uppercase align-top">
            Layer
          </th>
        </tr>
      </thead>
      <tbody>
        {items.map((item) => {
          const hidden =
            visibleIds != null && !visibleIds.has(item.id) ? 'true' : undefined
          return (
            <tr
              key={item.id}
              data-hidden={hidden}
              className="data-[hidden=true]:hidden"
            >
              <td className="border-b border-border px-3 py-2.5 align-top">
                <Link to={`/rfc/${item.id}`} className="font-semibold no-underline">
                  RFC-{item.id}
                </Link>
              </td>
              <td className="border-b border-border px-3 py-2.5 align-top">
                <Link to={`/rfc/${item.id}`} className="font-semibold no-underline">
                  {item.title}
                </Link>
              </td>
              <td className="border-b border-border px-3 py-2.5 align-top">
                <StatusBadge status={item.status} />
              </td>
              <td className="border-b border-border px-3 py-2.5 align-top">
                <LayerChip layer={item.layer} />
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
