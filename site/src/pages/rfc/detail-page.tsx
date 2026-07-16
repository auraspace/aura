import { useEffect } from 'react'
import { Link, useLocation, useParams } from 'react-router-dom'
import { getRfcById } from '@/lib/rfc/load-rfcs'
import { StatusBadge } from '@/pages/rfc/components/status-badge'
import { LayerChip } from '@/pages/rfc/components/layer-chip'
import { DepLinks } from '@/pages/rfc/components/dep-links'
import { Toc } from '@/pages/rfc/components/toc'
import { RfcArticle } from '@/pages/rfc/components/article'
import { NotFoundPage } from '@/pages/not-found-page'

export function DetailPage() {
  const { id } = useParams()
  const location = useLocation()
  const doc = id ? getRfcById(id) : undefined

  useEffect(() => {
    if (!location.hash) return
    const el = document.getElementById(location.hash.slice(1))
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }, [location.hash, doc?.id])

  if (!doc) return <NotFoundPage />

  return (
    <div>
      <p className="text-muted">
        <Link to="/rfc">← Catalog</Link>
      </p>
      <header className="mb-4">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <span className="inline-block rounded-full border border-border bg-card px-2 py-0.5 text-xs text-muted">
            RFC-{doc.id}
          </span>
          <StatusBadge status={doc.status} />
          <LayerChip layer={doc.layer} />
        </div>
        <h1 className="mt-1 mb-2 leading-tight">{doc.title}</h1>
        <DepLinks label="Depends" ids={doc.depends} />
        <DepLinks label="Blocks" ids={doc.blocks} />
      </header>
      <div className="grid grid-cols-1 items-start gap-6 rfc:grid-cols-[220px_1fr]">
        <Toc headings={doc.headings} />
        <RfcArticle doc={doc} />
      </div>
    </div>
  )
}
