import type { Heading } from '@/lib/rfc/types'

function scrollToId(id: string) {
  const el = document.getElementById(id)
  if (!el) return false
  el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  // Keep URL hash in sync without full navigation
  history.replaceState(null, '', `#${id}`)
  return true
}

export function Toc({ headings }: { headings: Heading[] }) {
  if (!headings.length) return null

  return (
    <nav
      className="sticky top-16 max-h-[calc(100vh-5rem)] overflow-auto rounded-lg border border-border bg-card px-4 py-3 text-sm"
      aria-label="Table of contents"
    >
      <p className="mb-2 text-xs font-semibold tracking-wide text-muted uppercase">
        On this page
      </p>
      <ul className="m-0 list-none p-0">
        {headings.map((h, index) => (
          <li
            key={`${h.id}-${index}`}
            className={
              h.depth === 3
                ? 'my-1 pl-3 text-xs text-muted'
                : 'my-1'
            }
          >
            <a
              href={`#${h.id}`}
              className="text-fg no-underline hover:text-accent"
              onClick={(e) => {
                if (scrollToId(h.id)) e.preventDefault()
              }}
            >
              {h.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  )
}
