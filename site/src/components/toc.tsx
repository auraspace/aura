import type { Heading } from '@/types/rfc'

export function Toc({ headings }: { headings: Heading[] }) {
  if (!headings.length) return null

  return (
    <nav className="toc" aria-label="Table of contents">
      <h2>On this page</h2>
      <ul>
        {headings.map((h) => (
          <li key={h.id} className={`depth-${h.depth}`}>
            <a href={`#${h.id}`}>{h.text}</a>
          </li>
        ))}
      </ul>
    </nav>
  )
}
