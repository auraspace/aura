import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeSlug from 'rehype-slug'
import { Link } from 'react-router-dom'
import type { Components } from 'react-markdown'
import type { RfcDoc } from '@/types/rfc'
import { linkifyRfcRefs } from '@/lib/links'

function normalizeHref(href: string | undefined): string | undefined {
  if (!href) return href
  if (href.startsWith('rfc/')) return `/${href}`
  const md = href.match(/RFC-(\d{3})/i)
  if (md && href.endsWith('.md')) return `/rfc/${md[1]}`
  return href
}

const components: Components = {
  a({ href, children }) {
    const to = normalizeHref(href)
    if (to?.startsWith('/rfc/')) {
      return <Link to={to}>{children}</Link>
    }
    if (to?.startsWith('#')) {
      return <a href={to}>{children}</a>
    }
    return (
      <a href={to} rel="noreferrer" target={to?.startsWith('http') ? '_blank' : undefined}>
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
