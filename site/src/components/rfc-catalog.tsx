import { Link } from 'react-router-dom'
import type { RfcMeta } from '@/types/rfc'
import { StatusBadge } from './status-badge'
import { LayerChip } from './layer-chip'

interface RfcCatalogProps {
  items: RfcMeta[]
  /** When set, rows whose id is not in the set get data-hidden (client filter). */
  visibleIds?: Set<string> | null
}

export function RfcCatalog({ items, visibleIds }: RfcCatalogProps) {
  return (
    <table className="catalog">
      <thead>
        <tr>
          <th>RFC</th>
          <th>Title</th>
          <th>Status</th>
          <th>Layer</th>
        </tr>
      </thead>
      <tbody>
        {items.map((item) => {
          const hidden =
            visibleIds != null && !visibleIds.has(item.id) ? 'true' : undefined
          return (
            <tr key={item.id} data-hidden={hidden}>
              <td>
                <Link to={`/rfc/${item.id}`}>RFC-{item.id}</Link>
              </td>
              <td>
                <Link to={`/rfc/${item.id}`}>{item.title}</Link>
              </td>
              <td>
                <StatusBadge status={item.status} />
              </td>
              <td>
                <LayerChip layer={item.layer} />
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
