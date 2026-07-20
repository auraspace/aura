import MiniSearch from 'minisearch'

import type { RfcDoc } from '@/lib/rfc/types'

export function buildSearchIndex(docs: RfcDoc[]) {
  const mini = new MiniSearch({
    fields: ['title', 'status', 'layer', 'body'],
    storeFields: ['id'],
    idField: 'id',
  })
  mini.addAll(
    docs.map((d) => ({
      id: d.id,
      title: d.title,
      status: d.status,
      layer: d.layer,
      body: d.markdown.replace(/```[\s\S]*?```/g, ' ').slice(0, 20000),
    })),
  )
  return mini
}
