import { IconArrowRight, IconBook2 } from '@tabler/icons-react'
import { Link } from 'react-router-dom'
import { getAllGuideMeta, getGuideNav } from '@/lib/docs'
import { DocsLayout } from './components/layout'

export function DocsHubPage() {
  const nav = getGuideNav()
  const items = getAllGuideMeta()
  const first = items[0]

  return (
    <DocsLayout nav={nav}>
      <p className="eyebrow">User guide</p>
      <h1 className="mt-3 font-display text-[34px] font-medium tracking-tight md:text-[42px]">
        Documentation
      </h1>
      <p className="mt-3 max-w-[540px] text-[17px] leading-[1.55] text-muted">
        Practical guides for learning and using Aura. Design decisions and
        contracts live in the{' '}
        <Link to="/rfc" className="font-medium">
          RFC catalog
        </Link>
        . Use search in the sidebar to jump by keyword.
      </p>

      {first && (
        <div className="mt-8 flex flex-wrap gap-3">
          <Link to={`/docs/${first.slug}`} className="btn-primary">
            <IconBook2 size={16} stroke={1.75} aria-hidden />
            Start with {first.title}
            <IconArrowRight size={16} stroke={1.75} aria-hidden />
          </Link>
          <Link to="/docs/language-tour" className="btn-ghost">
            Language tour
            <IconArrowRight size={15} stroke={1.75} aria-hidden />
          </Link>
          <Link to="/docs/syntax-cheatsheet" className="btn-ghost">
            Cheatsheet
            <IconArrowRight size={15} stroke={1.75} aria-hidden />
          </Link>
        </div>
      )}

      <div className="mt-12 space-y-10">
        {nav.map((section) => (
          <section key={section.title}>
            <h2 className="font-display text-[22px] font-medium tracking-tight">
              {section.title}
            </h2>
            <ul className="mt-4 m-0 grid list-none gap-3 p-0 sm:grid-cols-2">
              {section.items.map((item) => (
                <li key={item.slug}>
                  <Link
                    to={`/docs/${item.slug}`}
                    className="block rounded-2xl border border-border bg-card p-5 text-fg no-underline transition-shadow duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] hover:shadow-[var(--lift)]"
                  >
                    <span className="font-display text-[18px] font-medium tracking-tight">
                      {item.title}
                    </span>
                    {item.summary ? (
                      <span className="mt-2 block text-[14px] leading-[1.5] text-muted">
                        {item.summary}
                      </span>
                    ) : null}
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </DocsLayout>
  )
}
