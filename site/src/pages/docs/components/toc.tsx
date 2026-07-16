import { HeadingLabel } from '@/components/markdown/heading-label'
import type { GuideHeading } from '@/lib/docs'

function scrollToId(id: string) {
  const el = document.getElementById(id)
  if (!el) return false
  el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  history.replaceState(null, '', `#${id}`)
  return true
}

export function DocsToc({ headings }: { headings: GuideHeading[] }) {
  if (!headings.length) return null

  return (
    <nav
      className="sticky top-24 max-h-[calc(100vh-7rem)] overflow-auto rounded-2xl border border-border bg-card px-4 py-3 text-sm"
      aria-label="On this page"
    >
      <p className="eyebrow mb-3">On this page</p>
      <ul className="m-0 list-none p-0">
        {headings.map((h, index) => (
          <li
            key={`${h.id}-${index}`}
            className={h.depth === 3 ? 'my-1 pl-3 text-xs text-muted' : 'my-1.5'}
          >
            <a
              href={`#${h.id}`}
              className="text-fg no-underline hover:text-accent"
              onClick={(e) => {
                if (scrollToId(h.id)) e.preventDefault()
              }}
            >
              <HeadingLabel text={h.text} />
            </a>
          </li>
        ))}
      </ul>
    </nav>
  )
}
