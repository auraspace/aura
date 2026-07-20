import { createHighlighter, type Highlighter } from 'shiki'

const LANG_MAP: Record<string, string> = {
  aura: 'kotlin',
  kt: 'kotlin',
  rs: 'rust',
  rust: 'rust',
  ts: 'typescript',
  typescript: 'typescript',
  js: 'javascript',
  javascript: 'javascript',
  tsx: 'tsx',
  jsx: 'jsx',
  toml: 'toml',
  bash: 'bash',
  sh: 'bash',
  shell: 'bash',
  zsh: 'bash',
  json: 'json',
  md: 'markdown',
  markdown: 'markdown',
  c: 'c',
  text: 'text',
  plain: 'text',
  plaintext: 'text',
}

const LANGS = [
  'kotlin',
  'rust',
  'typescript',
  'javascript',
  'tsx',
  'jsx',
  'toml',
  'bash',
  'json',
  'markdown',
  'c',
  'text',
  'java',
] as const

let highlighter: Highlighter | null = null
let initPromise: Promise<Highlighter> | null = null

export function ensureHighlighter(): Promise<Highlighter> {
  if (highlighter) return Promise.resolve(highlighter)
  if (!initPromise) {
    initPromise = createHighlighter({
      themes: ['github-light', 'github-dark'],
      langs: [...LANGS],
    }).then((h) => {
      highlighter = h
      return h
    })
  }
  return initPromise
}

export function resolveLang(lang: string | undefined): string {
  if (!lang) return 'text'
  const key = lang.toLowerCase().trim()
  return (
    LANG_MAP[key] ??
    (LANGS.includes(key as (typeof LANGS)[number]) ? key : 'text')
  )
}

/** Sync highlight after ensureHighlighter() has resolved. */
export function highlightCode(code: string, lang?: string): string {
  const h = highlighter
  if (!h) {
    const escaped = code
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
    return `<pre class="shiki shiki-fallback"><code>${escaped}</code></pre>`
  }

  const language = resolveLang(lang)
  try {
    return h.codeToHtml(code, {
      lang: language,
      themes: {
        light: 'github-light',
        dark: 'github-dark',
      },
      defaultColor: false,
    })
  } catch {
    const escaped = code
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
    return `<pre class="shiki shiki-fallback"><code>${escaped}</code></pre>`
  }
}

export function isHighlighterReady(): boolean {
  return highlighter != null
}
