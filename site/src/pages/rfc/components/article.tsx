import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeSlug from 'rehype-slug'
import { Link } from 'react-router-dom'
import type { Components } from 'react-markdown'
import type { RfcDoc } from '@/lib/rfc/types'
import { linkifyRfcRefs } from '@/lib/rfc/links'

function normalizeHref(href: string | undefined): string | undefined {
  if (!href) return href
  if (href.startsWith('rfc/')) return `/${href}`
  const md = href.match(/RFC-(\d{3})/i)
  if (md && href.endsWith('.md')) return `/rfc/${md[1]}`
  return href
}

/**
 * Heading ids come from rehype-slug (github-slugger) — applied once on the
 * markdown AST, so React Strict Mode re-renders cannot skip / exhaust ids.
 * Must match `extractHeadings` in parse-rfc.ts (also github-slugger).
 */
const components: Components = {
  // Only forward real DOM attrs (id). Never spread `node` / react-markdown props.
  h2: ({ children, id }) => <h2 id={id}>{children}</h2>,
  h3: ({ children, id }) => <h3 id={id}>{children}</h3>,
  a({ href, children }) {
    const to = normalizeHref(href)
    if (to?.startsWith('/rfc/')) {
      return <Link to={to}>{children}</Link>
    }
    if (to?.startsWith('#')) {
      return (
        <a
          href={to}
          onClick={(e) => {
            const id = decodeURIComponent(to.slice(1))
            const el = document.getElementById(id)
            if (!el) return
            e.preventDefault()
            el.scrollIntoView({ behavior: 'smooth', block: 'start' })
            history.replaceState(null, '', to)
          }}
        >
          {children}
        </a>
      )
    }
    return (
      <a
        href={to}
        rel="noreferrer"
        target={to?.startsWith('http') ? '_blank' : undefined}
      >
        {children}
      </a>
    )
  },
}

export function RfcArticle({ doc }: { doc: RfcDoc }) {
  const markdown = linkifyRfcRefs(doc.markdown)

  return (
    <article className="rfc-article">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSlug]}
        components={components}
      >
        {markdown}
      </ReactMarkdown>
    </article>
  )
}
