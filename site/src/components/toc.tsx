import type { Heading } from '@/types/rfc'

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
    <nav className="toc" aria-label="Table of contents">
      <h2>On this page</h2>
      <ul>
        {headings.map((h, index) => (
          <li key={`${h.id}-${index}`} className={`depth-${h.depth}`}>
            <a
              href={`#${h.id}`}
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
