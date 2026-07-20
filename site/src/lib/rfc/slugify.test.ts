import { describe, expect, it } from 'vitest'

import { slugify } from './slugify'

describe('slugify', () => {
  it('lowercases and hyphenates', () => {
    expect(slugify('Vision & Design Principles')).toBe(
      'vision-design-principles',
    )
  })

  it('strips punctuation', () => {
    expect(slugify('1. Abstract')).toBe('1-abstract')
  })

  it('collapses whitespace', () => {
    expect(slugify('  Hello   World  ')).toBe('hello-world')
  })
})
