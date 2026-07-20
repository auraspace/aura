import type { ReactNode } from 'react'

import type { GuideNavSection } from '@/lib/docs'

import { DocsSearchBox } from './search-box'
import { DocsSidebar } from './sidebar'

export function DocsLayout({
  nav,
  activeSlug,
  children,
  toc,
}: {
  nav: GuideNavSection[]
  activeSlug?: string
  children: ReactNode
  toc?: ReactNode
}) {
  return (
    <main className="page-shell !max-w-[1280px]">
      <div className="mb-8 max-w-xl lg:hidden">
        <DocsSearchBox />
      </div>

      <div className="grid grid-cols-1 gap-8 lg:grid-cols-[220px_minmax(0,1fr)] xl:grid-cols-[220px_minmax(0,1fr)_200px]">
        <div className="hidden lg:block">
          <div className="mb-6">
            <DocsSearchBox />
          </div>
          <DocsSidebar nav={nav} activeSlug={activeSlug} />
        </div>

        <div className="min-w-0">{children}</div>

        <div className="hidden xl:block">{toc}</div>
      </div>

      <details className="mt-10 rounded-2xl border border-border bg-card p-4 lg:hidden">
        <summary className="cursor-pointer font-medium text-fg">
          All docs pages
        </summary>
        <div className="mt-4">
          <DocsSidebar nav={nav} activeSlug={activeSlug} />
        </div>
      </details>
    </main>
  )
}
