import { useMemo, useState } from 'react'
import { getAllMeta, getAllRfcs } from '@/lib/rfc/load-rfcs'
import { buildSearchIndex } from '@/lib/rfc/search'
import { RfcCatalog } from '@/pages/rfc/components/catalog'
import { FilterBar } from '@/pages/rfc/components/filter-bar'
import { SearchBox } from '@/pages/rfc/components/search-box'

export function CatalogPage() {
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

    if (!status && !layer && !query.trim()) return null
    return ids
  }, [items, status, layer, query, index])

  return (
    <main className="page-shell">
      <p className="eyebrow">Language & toolchain</p>
      <h1 className="mt-3 font-display text-[34px] font-medium tracking-tight md:text-[40px]">
        RFC catalog
      </h1>
      <p className="mt-2 max-w-[520px] text-muted">
        Internal index of language and toolchain RFCs ({items.length} documents).
      </p>
      <div className="my-6 flex flex-wrap items-end gap-3">
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
    </main>
  )
}
