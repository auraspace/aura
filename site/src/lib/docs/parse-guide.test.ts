import { describe, expect, it } from 'vitest'

import {
  parseFrontmatter,
  parseGuideMarkdown,
  stripLeadingH1,
} from './parse-guide'

const SAMPLE = `---
title: Getting started
section: Start
order: 20
summary: Clone and run hello.
---

# Getting started

Intro paragraph.

## Prerequisites

Need Rust.

### Optional

pnpm for the site.
`

describe('parseFrontmatter', () => {
  it('parses key/value fields and body', () => {
    const { fields, body } = parseFrontmatter(SAMPLE)
    expect(fields.title).toBe('Getting started')
    expect(fields.section).toBe('Start')
    expect(fields.order).toBe('20')
    expect(body.startsWith('# Getting started')).toBe(true)
  })
})

describe('parseGuideMarkdown', () => {
  it('builds meta, strips H1, extracts headings', () => {
    const doc = parseGuideMarkdown(SAMPLE, 'getting-started.md')
    expect(doc.slug).toBe('getting-started')
    expect(doc.title).toBe('Getting started')
    expect(doc.section).toBe('Start')
    expect(doc.order).toBe(20)
    expect(doc.summary).toBe('Clone and run hello.')
    expect(doc.markdown.startsWith('Intro paragraph')).toBe(true)
    expect(doc.headings.map((h) => h.id)).toEqual(['prerequisites', 'optional'])
    expect(doc.headings[0].depth).toBe(2)
    expect(doc.headings[1].depth).toBe(3)
  })

  it('falls back to filename slug when frontmatter omitted', () => {
    const doc = parseGuideMarkdown(
      '# Only title\n\nBody.\n',
      'language-tour.md',
    )
    expect(doc.slug).toBe('language-tour')
    expect(doc.title).toBe('language-tour')
  })
})

describe('stripLeadingH1', () => {
  it('removes first ATX h1 only', () => {
    expect(stripLeadingH1('# Title\n\n## Next\n')).toBe('## Next\n')
  })
})
