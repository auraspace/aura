import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Link } from 'react-router-dom'
import type { Components } from 'react-markdown'
import type { ReactNode } from 'react'
import type { RfcDoc } from '@/types/rfc'
import { linkifyRfcRefs } from '@/lib/links'

function normalizeHref(href: string | undefined): string | undefined {
  if (!href) return href
  if (href.startsWith('rfc/')) return `/${href}`
  const md = href.match(/RFC-(\d{3})/i)
  if (md && href.endsWith('.md')) return `/rfc/${md[1]}`
  return href
}

function createComponents(headingIds: string[]): Components {
  let headingIndex = 0
  const nextHeadingId = () => headingIds[headingIndex++]

  const heading =
    (Tag: 'h2' | 'h3') =>
    ({ children, ...props }: { children?: ReactNode }) => {
      const id = nextHeadingId()
      return (
        <Tag id={id} {...props}>
          {children}
        </Tag>
      )
    }

  return {
    h2: heading('h2'),
    h3: heading('h3'),
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
}

export function RfcArticle({ doc }: { doc: RfcDoc }) {
  const markdown = linkifyRfcRefs(doc.markdown)
  const components = createComponents(doc.headings.map((h) => h.id))

  return (
    <article className="rfc-article">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {markdown}
      </ReactMarkdown>
    </article>
  )
}
