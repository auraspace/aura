import { useMemo, useState } from 'react'
import { getAllMeta, getAllRfcs } from '@/lib/load-rfcs'
import { buildSearchIndex } from '@/lib/search'
import { RfcCatalog } from '@/components/rfc-catalog'
import { FilterBar } from '@/components/filter-bar'
import { SearchBox } from '@/components/search-box'

export function HomePage() {
  const items = getAllMeta()
  const docs = getAllRfcs()
  const [status, setStatus] = useState('')
  const [layer, setLayer] = useState('')
  const [query, setQuery] = useState('')

  const layers = useMemo(
    () => [...new Set(items.map((i) => i.layer))].sort(),
    [items],
  )

  const index = useMemo(() => buildSearchIndex(docs), [docs])

  const visibleIds = useMemo(() => {
    let ids = new Set(items.map((i) => i.id))

    if (status) {
      ids = new Set(
        items.filter((i) => i.status === status && ids.has(i.id)).map((i) => i.id),
      )
    }
    if (layer) {
      ids = new Set(
        items.filter((i) => i.layer === layer && ids.has(i.id)).map((i) => i.id),
      )
    }
    if (query.trim()) {
      const hits = index.search(query.trim(), { prefix: true, fuzzy: 0.2 })
      const hitIds = new Set(hits.map((h) => String(h.id)))
      ids = new Set([...ids].filter((id) => hitIds.has(id)))
    }

    // null means “no client filter yet” only when all empty — always pass set
    // so SSR and client show same rows when filters empty
    if (!status && !layer && !query.trim()) return null
    return ids
  }, [items, status, layer, query, index])

  return (
    <div>
      <h1>Aura RFC catalog</h1>
      <p className="muted">
        Internal index of language and toolchain RFCs ({items.length} documents).
      </p>
      <div className="toolbar">
        <SearchBox value={query} onChange={setQuery} />
        <FilterBar
          status={status}
          layer={layer}
          layers={layers}
          onStatusChange={setStatus}
          onLayerChange={setLayer}
        />
      </div>
      <RfcCatalog items={items} visibleIds={visibleIds} />
    </div>
  )
}
