import { IconSearch, IconX } from '@tabler/icons-react'
import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { getAllGuides } from '@/lib/docs'
import { buildGuideSearchIndex, searchGuides } from '@/lib/docs/search'

export function DocsSearchBox({ className = '' }: { className?: string }) {
  const docs = getAllGuides()
  const index = useMemo(() => buildGuideSearchIndex(docs), [docs])
  const [query, setQuery] = useState('')

  const hits = useMemo(() => searchGuides(index, query), [index, query])

  return (
    <div className={className}>
      <label className="relative block">
        <span className="sr-only">Search docs</span>
        <IconSearch
          size={16}
          stroke={1.75}
          className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted"
          aria-hidden
        />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search docs…"
          className="w-full rounded-full border border-border-strong bg-card py-2.5 pr-10 pl-10 text-[14px] text-fg outline-none transition-colors placeholder:text-ink-muted focus:border-accent"
        />
        {query ? (
          <button
            type="button"
            className="absolute top-1/2 right-2.5 grid h-7 w-7 -translate-y-1/2 place-items-center rounded-full text-muted hover:bg-tint hover:text-fg"
            onClick={() => setQuery('')}
            aria-label="Clear search"
          >
            <IconX size={14} stroke={1.75} />
          </button>
        ) : null}
      </label>

      {query.trim() ? (
        <ul className="mt-3 m-0 max-h-80 list-none space-y-1 overflow-auto rounded-2xl border border-border bg-card p-2">
          {hits.length === 0 ? (
            <li className="px-3 py-4 text-[14px] text-muted">No matches.</li>
          ) : (
            hits.map((hit) => (
              <li key={hit.slug}>
                <Link
                  to={`/docs/${hit.slug}`}
                  className="block rounded-xl px-3 py-2.5 text-fg no-underline transition-colors hover:bg-tint"
                  onClick={() => setQuery('')}
                >
                  <span className="flex items-baseline justify-between gap-3">
                    <span className="font-medium">{hit.title}</span>
                    <span className="font-mono text-[10px] tracking-[0.1em] text-ink-muted uppercase">
                      {hit.section}
                    </span>
                  </span>
                  {hit.summary ? (
                    <span className="mt-0.5 block text-[13px] text-muted line-clamp-2">
                      {hit.summary}
                    </span>
                  ) : null}
                </Link>
              </li>
            ))
          )}
        </ul>
      ) : null}
    </div>
  )
}
