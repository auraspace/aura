import type { HighlighterCore as Highlighter } from '@shikijs/core'
import githubDark from '@shikijs/themes/github-dark'
import githubLight from '@shikijs/themes/github-light'

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
    initPromise = Promise.all([
      import('shiki/core'),
      import('@shikijs/engine-javascript'),
      import('@shikijs/langs/bash'),
      import('@shikijs/langs/c'),
      import('@shikijs/langs/java'),
      import('@shikijs/langs/javascript'),
      import('@shikijs/langs/json'),
      import('@shikijs/langs/jsx'),
      import('@shikijs/langs/kotlin'),
      import('@shikijs/langs/markdown'),
      import('@shikijs/langs/rust'),
      import('@shikijs/langs/toml'),
      import('@shikijs/langs/tsx'),
      import('@shikijs/langs/typescript'),
    ])
      .then(([core, engine, ...languages]) =>
        core.createHighlighterCore({
          engine: engine.createJavaScriptRegexEngine(),
          themes: [githubLight, githubDark],
          langs: languages.flatMap(({ default: language }) => language),
        }),
      )
      .then((h) => {
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
