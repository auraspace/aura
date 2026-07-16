import { parseInlineMarkdown } from '@/lib/markdown/heading-text'

/** Render lightweight inline Markdown inside TOC / labels. */
export function HeadingLabel({ text }: { text: string }) {
  const parts = parseInlineMarkdown(text)
  return (
    <>
      {parts.map((p, i) => {
        switch (p.type) {
          case 'code':
            return (
              <code
                key={i}
                className="rounded bg-tint px-1 py-0.5 font-mono text-[0.85em] text-fg"
              >
                {p.value}
              </code>
            )
          case 'strong':
            return <strong key={i}>{p.value}</strong>
          case 'em':
            return <em key={i}>{p.value}</em>
          case 'link':
            return (
              <span key={i} className="underline decoration-border-strong">
                {p.value}
              </span>
            )
          default:
            return <span key={i}>{p.value}</span>
        }
      })}
    </>
  )
}
