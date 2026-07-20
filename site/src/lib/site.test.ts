import { describe, expect, it } from 'vitest'

import { absoluteUrl, escapeHtmlAttr, SITE_ORIGIN } from './site'

describe('absoluteUrl', () => {
  it('uses production origin and root slash for home', () => {
    expect(absoluteUrl('/')).toBe(`${SITE_ORIGIN}/`)
    expect(absoluteUrl('')).toBe(`${SITE_ORIGIN}/`)
  })

  it('strips trailing slashes on non-root paths', () => {
    expect(absoluteUrl('/docs')).toBe(`${SITE_ORIGIN}/docs`)
    expect(absoluteUrl('/docs/')).toBe(`${SITE_ORIGIN}/docs`)
    expect(absoluteUrl('/rfc/000')).toBe(`${SITE_ORIGIN}/rfc/000`)
  })

  it('includes optional base path', () => {
    expect(absoluteUrl('/', '/aura/')).toBe(`${SITE_ORIGIN}/aura/`)
    expect(absoluteUrl('/docs', '/aura')).toBe(`${SITE_ORIGIN}/aura/docs`)
  })
})

describe('escapeHtmlAttr', () => {
  it('escapes attribute-sensitive characters', () => {
    expect(escapeHtmlAttr('a & b "c" <d>')).toBe(
      'a &amp; b &quot;c&quot; &lt;d&gt;',
    )
  })
})
