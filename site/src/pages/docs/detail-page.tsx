import { IconArrowLeft, IconArrowRight } from '@tabler/icons-react'
import { useEffect } from 'react'
import { Link, useLocation, useParams } from 'react-router-dom'

import { MarkdownActions } from '@/components/markdown/markdown-actions'
import { getAdjacentGuides, getGuideBySlug, getGuideNav } from '@/lib/docs'
import { NotFoundPage } from '@/pages/not-found-page'

import { GuideArticle } from './components/article'
import { DocsLayout } from './components/layout'
import { DocsToc } from './components/toc'

export function DocsDetailPage() {
  const { slug } = useParams()
  const location = useLocation()
  const doc = slug ? getGuideBySlug(slug) : undefined
  const nav = getGuideNav()

  useEffect(() => {
    if (!location.hash) return
    const el = document.getElementById(location.hash.slice(1))
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }, [location.hash, doc?.slug])

  if (!doc) return <NotFoundPage />

  const { prev, next } = getAdjacentGuides(doc.slug)

  return (
    <DocsLayout
      nav={nav}
      activeSlug={doc.slug}
      toc={<DocsToc headings={doc.headings} />}
    >
      <p className="text-[13px] text-muted">
        <Link to="/docs" className="navlink">
          Docs
        </Link>
        <span className="mx-2 text-ink-muted">/</span>
        <span className="text-fg">{doc.section}</span>
      </p>

      <header className="mb-6 mt-3">
        <p className="eyebrow">{doc.section}</p>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <h1 className="mt-2 font-display text-[32px] leading-tight font-medium tracking-tight md:text-[40px]">
            {doc.title}
          </h1>
          <MarkdownActions
            markdown={doc.markdown}
            githubPath={`docs/guide/${doc.fileName}`}
          />
        </div>
        {doc.summary ? (
          <p className="mt-3 max-w-[560px] text-[16px] leading-[1.55] text-muted">
            {doc.summary}
          </p>
        ) : null}
      </header>

      <GuideArticle doc={doc} />

      <nav
        className="mt-10 grid grid-cols-1 gap-3 border-t border-border pt-8 sm:grid-cols-2"
        aria-label="Adjacent pages"
      >
        {prev ? (
          <Link
            to={`/docs/${prev.slug}`}
            className="group flex items-start gap-3 rounded-2xl border border-border bg-card p-4 text-fg no-underline transition-colors hover:border-border-strong"
          >
            <IconArrowLeft
              size={18}
              stroke={1.75}
              className="mt-0.5 shrink-0 text-muted"
              aria-hidden
            />
            <span>
              <span className="block text-[12px] text-muted">Previous</span>
              <span className="mt-0.5 block font-medium group-hover:text-accent">
                {prev.title}
              </span>
            </span>
          </Link>
        ) : (
          <span />
        )}
        {next ? (
          <Link
            to={`/docs/${next.slug}`}
            className="group flex items-start justify-end gap-3 rounded-2xl border border-border bg-card p-4 text-right text-fg no-underline transition-colors hover:border-border-strong sm:col-start-2"
          >
            <span>
              <span className="block text-[12px] text-muted">Next</span>
              <span className="mt-0.5 block font-medium group-hover:text-accent">
                {next.title}
              </span>
            </span>
            <IconArrowRight
              size={18}
              stroke={1.75}
              className="mt-0.5 shrink-0 text-muted"
              aria-hidden
            />
          </Link>
        ) : null}
      </nav>
    </DocsLayout>
  )
}
