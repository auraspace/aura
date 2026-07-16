import GithubSlugger from 'github-slugger'
import { plainHeadingText } from '@/lib/markdown/heading-text'
import type { RfcDoc, RfcStatus } from '@/lib/rfc/types'
import { slugify } from './slugify'

const VALID_STATUS = new Set<RfcStatus>([
  'Draft',
  'In Review',
  'Accepted',
  'Frozen',
  'Rejected',
  'Superseded',
])

function padId(n: number): string {
  return String(n).padStart(3, '0')
}

/** Parse Depends/Blocks cell into zero-padded ids. */
export function parseDependsList(raw: string): string[] {
  const text = raw.replace(/\u2026/g, '…').trim()
  if (!text || text === '—' || text === '-' || text === '–') return []

  const range = text.match(/RFC-(\d+)\s*[….]+?\s*RFC-(\d+)/i)
  if (range) {
    const start = parseInt(range[1], 10)
    const end = parseInt(range[2], 10)
    const out: string[] = []
    for (let i = start; i <= end; i++) out.push(padId(i))
    return out
  }

  const ids = [...text.matchAll(/RFC-(\d+)/gi)].map((m) =>
    padId(parseInt(m[1], 10)),
  )
  if (ids.length) return [...new Set(ids)]

  return [...text.matchAll(/\b(\d{1,3})\b/g)].map((m) =>
    padId(parseInt(m[1], 10)),
  )
}

function parseMetaTable(source: string): Record<string, string> {
  const fields: Record<string, string> = {}
  for (const line of source.split('\n')) {
    const m = line.match(/^\|\s*\*\*([^*]+)\*\*\s*\|\s*(.*?)\s*\|\s*$/)
    if (!m) continue
    fields[m[1].trim()] = m[2].trim()
  }
  return fields
}

function extractBody(source: string): string {
  const hr = source.indexOf('\n---\n')
  if (hr !== -1) return source.slice(hr + 5).trim() + '\n'

  const lines = source.split('\n')
  let lastTable = -1
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].startsWith('|')) lastTable = i
  }
  if (lastTable === -1) return source
  return lines.slice(lastTable + 1).join('\n').trim() + '\n'
}

/**
 * Heading ids must match what the article renderer assigns (github-slugger /
 * rehype-slug algorithm) so TOC and in-doc `#` links resolve.
 */
/**
 * Heading ids must match what the article renderer assigns (github-slugger)
 * so TOC and in-doc `#` links resolve.
 */
function extractHeadings(markdown: string) {
  const slugger = new GithubSlugger()
  const headings: { depth: number; text: string; id: string }[] = []
  let inFence = false
  for (const line of markdown.split('\n')) {
    if (/^```/.test(line)) {
      inFence = !inFence
      continue
    }
    if (inFence) continue
    const m = line.match(/^(#{2,3})\s+(.+)$/)
    if (!m) continue
    const text = m[2].replace(/\s+#+\s*$/, '').trim()
    const plain = plainHeadingText(text)
    headings.push({ depth: m[1].length, text, id: slugger.slug(plain) })
  }
  return headings
}

export function parseRfcMarkdown(source: string, fileName: string): RfcDoc {
  const fields = parseMetaTable(source)
  const idRaw = fields['RFC']
  if (!idRaw) throw new Error(`Missing **RFC** field in ${fileName}`)
  const id = padId(parseInt(idRaw.replace(/\D/g, ''), 10))
  if (Number.isNaN(parseInt(id, 10))) {
    throw new Error(`Invalid RFC id in ${fileName}`)
  }

  const statusRaw = fields['Status']
  if (!statusRaw) throw new Error(`Missing **Status** field in ${fileName}`)
  if (!VALID_STATUS.has(statusRaw as RfcStatus)) {
    throw new Error(`Invalid status "${statusRaw}" in ${fileName}`)
  }

  const title =
    fields['Title'] ||
    source.match(/^#\s+RFC-\d+:\s*(.+)$/m)?.[1]?.trim() ||
    fileName

  const markdown = extractBody(source)
  const baseSlug = fileName.replace(/\.md$/i, '').toLowerCase()

  return {
    id,
    slug: baseSlug.startsWith('rfc-')
      ? baseSlug
      : `rfc-${id}-${slugify(title)}`,
    title,
    status: statusRaw as RfcStatus,
    layer: fields['Layer'] || 'Unknown',
    authors: (fields['Authors'] || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
    created: fields['Created'] || undefined,
    updated: fields['Updated'] || undefined,
    estimate: fields['Estimate'] || undefined,
    depends: parseDependsList(fields['Depends'] || ''),
    blocks: parseDependsList(fields['Blocks'] || ''),
    fileName,
    markdown,
    headings: extractHeadings(markdown),
  }
}
