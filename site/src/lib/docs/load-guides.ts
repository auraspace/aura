import { parseGuideMarkdown } from './parse-guide'
import type { GuideDoc, GuideMeta, GuideNavSection } from './types'

const modules = import.meta.glob('../../../../docs/guide/*.md', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

function fileNameFromPath(p: string): string {
  return p.split('/').pop() || p
}

export function loadAllGuides(): GuideDoc[] {
  const docs: GuideDoc[] = []
  for (const [path, source] of Object.entries(modules)) {
    const fileName = fileNameFromPath(path)
    if (!fileName.endsWith('.md')) continue
    docs.push(parseGuideMarkdown(source, fileName))
  }
  docs.sort((a, b) => a.order - b.order || a.title.localeCompare(b.title))
  return docs
}

let cache: GuideDoc[] | null = null

export function getAllGuides(): GuideDoc[] {
  if (!cache) cache = loadAllGuides()
  return cache
}

export function getGuideBySlug(slug: string): GuideDoc | undefined {
  return getAllGuides().find((g) => g.slug === slug)
}

export function getAllGuideMeta(): GuideMeta[] {
  return getAllGuides().map(({ markdown: _m, headings: _h, ...meta }) => meta)
}

/** Group guides by section, preserving global order within each section. */
export function getGuideNav(): GuideNavSection[] {
  const meta = getAllGuideMeta()
  const order: string[] = []
  const map = new Map<string, GuideMeta[]>()

  for (const item of meta) {
    if (!map.has(item.section)) {
      map.set(item.section, [])
      order.push(item.section)
    }
    map.get(item.section)!.push(item)
  }

  return order.map((title) => ({
    title,
    items: map.get(title)!,
  }))
}

export function getAdjacentGuides(slug: string): {
  prev?: GuideMeta
  next?: GuideMeta
} {
  const all = getAllGuideMeta()
  const i = all.findIndex((g) => g.slug === slug)
  if (i === -1) return {}
  return {
    prev: i > 0 ? all[i - 1] : undefined,
    next: i < all.length - 1 ? all[i + 1] : undefined,
  }
}
