import ReactMarkdown, { type Components } from 'react-markdown'
import { Link } from 'react-router-dom'
import rehypeSlug from 'rehype-slug'
import remarkGfm from 'remark-gfm'

import { markdownCodeComponents } from '@/components/markdown/code-block'
import type { GuideDoc } from '@/lib/docs'
import { linkifyRfcRefs } from '@/lib/rfc/links'

function normalizeHref(href: string | undefined): string | undefined {
  if (!href) return href
  if (href.startsWith('/')) return href
  if (href.startsWith('#')) return href

  const guide = href.match(/^(?:\.\.?\/)?([a-z0-9-]+)\.md(?:#(.+))?$/i)
  if (guide) {
    const base = `/docs/${guide[1]}`
    return guide[2] ? `${base}#${guide[2]}` : base
  }

  const rfc = href.match(/RFC-(\d{3})/i)
  if (rfc && href.endsWith('.md')) return `/rfc/${rfc[1]}`

  return href
}

const components: Components = {
  ...markdownCodeComponents,
  h2: ({ children, id }) => <h2 id={id}>{children}</h2>,
  h3: ({ children, id }) => <h3 id={id}>{children}</h3>,
  a({ href, children }) {
    const to = normalizeHref(href)
    if (to?.startsWith('/docs/') || to?.startsWith('/rfc/')) {
      const [path, hash] = to.split('#')
      return (
        <Link to={hash ? { pathname: path, hash: `#${hash}` } : path}>
          {children}
        </Link>
      )
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

export function GuideArticle({ doc }: { doc: GuideDoc }) {
  const markdown = linkifyRfcRefs(doc.markdown)

  return (
    <article className="rfc-article docs-article">
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
