import MiniSearch from 'minisearch'
import type { GuideDoc } from './types'

export type GuideSearchHit = {
  slug: string
  title: string
  section: string
  summary: string
}

export function buildGuideSearchIndex(docs: GuideDoc[]) {
  const mini = new MiniSearch({
    fields: ['title', 'section', 'summary', 'body'],
    storeFields: ['slug', 'title', 'section', 'summary'],
    idField: 'slug',
  })
  mini.addAll(
    docs.map((d) => ({
      slug: d.slug,
      title: d.title,
      section: d.section,
      summary: d.summary,
      body: d.markdown.replace(/```[\s\S]*?```/g, ' ').slice(0, 30000),
    })),
  )
  return mini
}

export function searchGuides(
  index: MiniSearch,
  query: string,
  limit = 12,
): GuideSearchHit[] {
  const q = query.trim()
  if (!q) return []
  return index
    .search(q, { prefix: true, fuzzy: 0.2 })
    .slice(0, limit)
    .map((hit) => ({
      slug: String(hit.slug ?? hit.id),
      title: String(hit.title ?? ''),
      section: String(hit.section ?? ''),
      summary: String(hit.summary ?? ''),
    }))
}
