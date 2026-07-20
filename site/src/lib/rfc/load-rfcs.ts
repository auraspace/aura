import type { RfcDoc, RfcMeta } from '@/lib/rfc/types'

import { parseRfcMarkdown } from './parse-rfc'

const modules = import.meta.glob('../../../../docs/rfc/RFC-*.md', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

function fileNameFromPath(p: string): string {
  return p.split('/').pop() || p
}

export function loadAllRfcs(): RfcDoc[] {
  const docs: RfcDoc[] = []
  for (const [path, source] of Object.entries(modules)) {
    const fileName = fileNameFromPath(path)
    if (!/^RFC-\d+/i.test(fileName)) continue
    docs.push(parseRfcMarkdown(source, fileName))
  }
  docs.sort((a, b) => a.id.localeCompare(b.id))
  return docs
}

let cache: RfcDoc[] | null = null

export function getAllRfcs(): RfcDoc[] {
  if (!cache) cache = loadAllRfcs()
  return cache
}

export function getRfcById(id: string): RfcDoc | undefined {
  const pad = id.padStart(3, '0')
  return getAllRfcs().find((r) => r.id === pad)
}

export function getAllMeta(): RfcMeta[] {
  return getAllRfcs().map(({ markdown: _m, headings: _h, ...meta }) => meta)
}
