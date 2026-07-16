import { Link } from 'react-router-dom'
import type { GuideNavSection } from '@/lib/docs'

export function DocsSidebar({
  nav,
  activeSlug,
}: {
  nav: GuideNavSection[]
  activeSlug?: string
}) {
  return (
    <nav
      className="sticky top-24 max-h-[calc(100vh-7rem)] overflow-auto pr-2"
      aria-label="Docs"
    >
      <p className="eyebrow mb-4">Documentation</p>
      <div className="space-y-6">
        {nav.map((section) => (
          <div key={section.title}>
            <p className="mb-2 font-mono text-[11px] font-medium tracking-[0.12em] text-ink-muted uppercase">
              {section.title}
            </p>
            <ul className="m-0 list-none space-y-0.5 p-0">
              {section.items.map((item) => {
                const active = item.slug === activeSlug
                return (
                  <li key={item.slug}>
                    <Link
                      to={`/docs/${item.slug}`}
                      className={[
                        'block rounded-lg px-2.5 py-1.5 text-[14px] no-underline transition-colors',
                        active
                          ? 'bg-tint font-medium text-fg'
                          : 'text-muted hover:bg-tint/70 hover:text-fg',
                      ].join(' ')}
                      aria-current={active ? 'page' : undefined}
                    >
                      {item.title}
                    </Link>
                  </li>
                )
              })}
            </ul>
          </div>
        ))}
      </div>
    </nav>
  )
}
