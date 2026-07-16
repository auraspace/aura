/** Turn bare RFC-NNN mentions into markdown links (skip existing links). */
export function linkifyRfcRefs(md: string): string {
  return md.replace(/\bRFC-(\d{3})\b/g, (full, id, offset, str) => {
    const before = str.slice(Math.max(0, offset - 1), offset)
    if (before === '[') return full
    // already part of markdown link URL
    const around = str.slice(Math.max(0, offset - 2), offset + full.length + 1)
    if (around.includes('](')) return full
    return `[${full}](/rfc/${id})`
  })
}
