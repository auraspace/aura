/**
 * Strip common inline Markdown so heading ids match rehype-slug /
 * github-slugger on rendered textContent (no `*[]()` markers).
 */
export function plainHeadingText(raw: string): string {
  let s = raw.replace(/#+\s*$/, '').trim()
  // images ![alt](url) → alt
  s = s.replace(/!\[([^\]]*)\]\([^)]+\)/g, '$1')
  // links [label](url) → label
  s = s.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
  // inline code
  s = s.replace(/`([^`]+)`/g, '$1')
  // bold / italic
  s = s.replace(/\*\*([^*]+)\*\*/g, '$1')
  s = s.replace(/__([^_]+)__/g, '$1')
  s = s.replace(/\*([^*]+)\*/g, '$1')
  s = s.replace(/_([^_]+)_/g, '$1')
  // strikethrough
  s = s.replace(/~~([^~]+)~~/g, '$1')
  return s.replace(/\s+/g, ' ').trim()
}

export type InlinePart =
  | { type: 'text'; value: string }
  | { type: 'code'; value: string }
  | { type: 'strong'; value: string }
  | { type: 'em'; value: string }
  | { type: 'link'; value: string; href: string }

/**
 * Tiny inline Markdown tokenizer for TOC labels (code, bold, italic, links).
 * Not a full MD parser — enough for heading lines in docs/RFCs.
 */
export function parseInlineMarkdown(raw: string): InlinePart[] {
  const input = raw.replace(/#+\s*$/, '').trim()
  const parts: InlinePart[] = []
  // Order: code, link, strong, em
  const re =
    /(`[^`]+`)|(\[[^\]]+\]\([^)]+\))|(\*\*[^*]+\*\*)|(__[^_]+__)|(\*[^*]+\*)|(_[^_]+_)/g
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(input))) {
    if (m.index > last) {
      parts.push({ type: 'text', value: input.slice(last, m.index) })
    }
    const token = m[0]
    if (token.startsWith('`')) {
      parts.push({ type: 'code', value: token.slice(1, -1) })
    } else if (token.startsWith('[')) {
      const lm = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/)
      if (lm) parts.push({ type: 'link', value: lm[1], href: lm[2] })
      else parts.push({ type: 'text', value: token })
    } else if (token.startsWith('**') || token.startsWith('__')) {
      parts.push({ type: 'strong', value: token.slice(2, -2) })
    } else {
      parts.push({ type: 'em', value: token.slice(1, -1) })
    }
    last = m.index + token.length
  }
  if (last < input.length) {
    parts.push({ type: 'text', value: input.slice(last) })
  }
  if (!parts.length) parts.push({ type: 'text', value: input })
  return parts
}
