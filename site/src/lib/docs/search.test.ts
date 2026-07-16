import { describe, expect, it } from 'vitest'
import { parseGuideMarkdown } from './parse-guide'
import { buildGuideSearchIndex, searchGuides } from './search'

const docs = [
  parseGuideMarkdown(
    `---
title: Getting started
section: Start
order: 20
summary: Clone and run hello.
---

# Getting started

Build the CLI with cargo and run corpus hello.
`,
    'getting-started.md',
  ),
  parseGuideMarkdown(
    `---
title: Arrays
section: Language
order: 38
summary: Array push pop iteration.
---

# Arrays

Use Array push and for-in loops.
`,
    'arrays.md',
  ),
]

describe('guide search', () => {
  it('finds by title and body', () => {
    const index = buildGuideSearchIndex(docs)
    const byTitle = searchGuides(index, 'arrays')
    expect(byTitle.some((h) => h.slug === 'arrays')).toBe(true)

    const byBody = searchGuides(index, 'cargo')
    expect(byBody.some((h) => h.slug === 'getting-started')).toBe(true)
  })

  it('returns empty for blank query', () => {
    const index = buildGuideSearchIndex(docs)
    expect(searchGuides(index, '   ')).toEqual([])
  })
})
