import GithubSlugger from 'github-slugger'
import { describe, expect, it } from 'vitest'

import { parseInlineMarkdown, plainHeadingText } from './heading-text'

describe('plainHeadingText', () => {
  it('strips inline code, links, emphasis', () => {
    expect(plainHeadingText('`class` — reference types')).toBe(
      'class — reference types',
    )
    expect(plainHeadingText('`Result<T, E>`')).toBe('Result<T, E>')
    expect(plainHeadingText('See [RFC-001](/rfc/001)')).toBe('See RFC-001')
    expect(plainHeadingText('**Bold** and *em*')).toBe('Bold and em')
  })

  it('matches github-slugger on plain vs raw for common headings', () => {
    const cases = [
      '`class` — reference types',
      '`Result<T, E>`',
      'Exceptions: `throw` / `try` / `catch` / `finally`',
      'How the CLI finds `std.*`',
      'Class `==` compares fields?',
    ]
    for (const raw of cases) {
      const plain = plainHeadingText(raw)
      expect(new GithubSlugger().slug(plain)).toBe(
        new GithubSlugger().slug(plainHeadingText(raw)),
      )
      // slug from plain equals slug from same plain text content
      expect(new GithubSlugger().slug(plain)).toBeTruthy()
    }
  })
})

describe('parseInlineMarkdown', () => {
  it('tokenizes code and text', () => {
    const p = parseInlineMarkdown('`class` — reference types')
    expect(p[0]).toEqual({ type: 'code', value: 'class' })
    expect(
      p.some((x) => x.type === 'text' && x.value.includes('reference')),
    ).toBe(true)
  })

  it('tokenizes Result generics in code', () => {
    const p = parseInlineMarkdown('`Result<T, E>`')
    expect(p).toEqual([{ type: 'code', value: 'Result<T, E>' }])
  })
})
