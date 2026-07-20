import { describe, expect, it } from 'vitest'

import { parseDependsList, parseRfcMarkdown } from './parse-rfc'

const SAMPLE = `# RFC-000: Vision & Design Principles

| Field        | Value                      |
| ------------ | -------------------------- |
| **RFC**      | 000                        |
| **Title**    | Vision & Design Principles |
| **Status**   | In Review                  |
| **Layer**    | Foundation                 |
| **Authors**  |                            |
| **Created**  | 2026-07-15                 |
| **Updated**  | 2026-07-15                 |
| **Estimate** | 15–20 pages                |
| **Depends**  | —                          |
| **Blocks**   | RFC-001 … RFC-003          |

---

## 1. Abstract

Hello body.

## 2. Motivation

More text.
`

describe('parseDependsList', () => {
  it('returns empty for em dash', () => {
    expect(parseDependsList('—')).toEqual([])
  })

  it('parses single RFC id', () => {
    expect(parseDependsList('RFC-004')).toEqual(['004'])
  })

  it('expands inclusive range with ellipsis', () => {
    expect(parseDependsList('RFC-001 … RFC-003')).toEqual(['001', '002', '003'])
  })

  it('parses comma-separated list', () => {
    expect(parseDependsList('RFC-001, RFC-002')).toEqual(['001', '002'])
  })
})

describe('parseRfcMarkdown', () => {
  it('extracts meta and body', () => {
    const doc = parseRfcMarkdown(SAMPLE, 'RFC-000-vision-design-principles.md')
    expect(doc.id).toBe('000')
    expect(doc.title).toBe('Vision & Design Principles')
    expect(doc.status).toBe('In Review')
    expect(doc.layer).toBe('Foundation')
    expect(doc.depends).toEqual([])
    expect(doc.blocks).toEqual(['001', '002', '003'])
    expect(doc.markdown).toContain('## 1. Abstract')
    expect(doc.markdown).not.toContain('**Status**')
    expect(doc.headings.map((h) => h.text)).toEqual([
      '1. Abstract',
      '2. Motivation',
    ])
    expect(doc.headings[0].id).toBe('1-abstract')
    expect(doc.slug).toBe('rfc-000-vision-design-principles')
  })

  it('uses github-slugger ids (ampersand → double hyphen)', () => {
    const md = `# RFC-099: X

| Field | Value |
| **RFC** | 099 |
| **Title** | X |
| **Status** | Draft |
| **Layer** | Language |
| **Depends** | — |
| **Blocks** | — |

---

## 5. Prior art & alternatives

### 6.4 Pillar map (language → ecosystem)

## 5. Prior art & alternatives
`
    const doc = parseRfcMarkdown(md, 'RFC-099-x.md')
    expect(doc.headings.map((h) => h.id)).toEqual([
      '5-prior-art--alternatives',
      '64-pillar-map-language--ecosystem',
      '5-prior-art--alternatives-1',
    ])
  })

  it('throws when RFC field missing', () => {
    const bad = `# Title\n\n| Field | Value |\n| **Status** | Draft |\n\n---\n\n## x\n`
    expect(() => parseRfcMarkdown(bad, 'bad.md')).toThrow(/RFC/)
  })
})
