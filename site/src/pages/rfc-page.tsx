import { useEffect } from 'react'
import { Link, useLocation, useParams } from 'react-router-dom'
import { getRfcById } from '@/lib/load-rfcs'
import { StatusBadge } from '@/components/status-badge'
import { LayerChip } from '@/components/layer-chip'
import { DepLinks } from '@/components/dep-links'
import { Toc } from '@/components/toc'
import { RfcArticle } from '@/components/rfc-article'
import { NotFoundPage } from './not-found-page'

export function RfcPage() {
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
      <p className="muted">
        <Link to="/">← Catalog</Link>
      </p>
      <header className="rfc-header">
        <div className="rfc-meta">
          <span className="badge">RFC-{doc.id}</span>
          <StatusBadge status={doc.status} />
          <LayerChip layer={doc.layer} />
        </div>
        <h1>{doc.title}</h1>
        <DepLinks label="Depends" ids={doc.depends} />
        <DepLinks label="Blocks" ids={doc.blocks} />
      </header>
      <div className="rfc-layout">
        <Toc headings={doc.headings} />
        <RfcArticle doc={doc} />
      </div>
    </div>
  )
}
