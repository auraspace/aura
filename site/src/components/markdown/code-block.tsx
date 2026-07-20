import { IconCheck, IconCopy } from '@tabler/icons-react'
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import type { Components } from 'react-markdown'

import { highlightCode } from '@/lib/highlight'

function extractText(node: unknown): string {
  if (node == null || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(extractText).join('')
  if (typeof node === 'object' && node !== null && 'props' in node) {
    const props = (node as { props?: { children?: unknown } }).props
    return extractText(props?.children)
  }
  return ''
}

async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    /* fall through */
  }
  try {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.setAttribute('readonly', '')
    ta.style.position = 'fixed'
    ta.style.left = '-9999px'
    document.body.appendChild(ta)
    ta.select()
    const ok = document.execCommand('copy')
    document.body.removeChild(ta)
    return ok
  } catch {
    return false
  }
}

function CodeBlockFrame({
  code,
  lang,
  html,
}: {
  code: string
  lang?: string
  html: string
}) {
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!copied) return
    const id = window.setTimeout(() => setCopied(false), 1600)
    return () => window.clearTimeout(id)
  }, [copied])

  const onCopy = useCallback(async () => {
    const ok = await copyText(code)
    if (ok) setCopied(true)
  }, [code])

  const label = lang && lang !== 'text' ? lang : 'code'

  return (
    <div className="code-block group relative my-4" data-language={label}>
      <div className="code-block-toolbar">
        <span className="code-block-lang">{label}</span>
        <button
          type="button"
          className="code-block-copy"
          onClick={onCopy}
          aria-label={copied ? 'Copied' : `Copy ${label}`}
        >
          {copied ? (
            <>
              <IconCheck size={14} stroke={1.75} aria-hidden />
              <span>Copied</span>
            </>
          ) : (
            <>
              <IconCopy size={14} stroke={1.75} aria-hidden />
              <span>Copy</span>
            </>
          )}
        </button>
      </div>
      <div className="shiki-wrap" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  )
}

/** Shared fenced-code renderer for docs + RFC articles (Shiki + copy). */
export const markdownCodeComponents: Pick<Components, 'code' | 'pre'> = {
  pre({ children }) {
    return <>{children}</>
  },
  code({ className, children }) {
    const text = extractText(children).replace(/\n$/, '')
    const match = /language-([\w+-]+)/.exec(className || '')

    // Inline code
    if (!match && !text.includes('\n')) {
      return <code className={className}>{children as ReactNode}</code>
    }

    const lang = match?.[1]
    const html = highlightCode(text, lang)
    return <CodeBlockFrame code={text} lang={lang} html={html} />
  },
}
