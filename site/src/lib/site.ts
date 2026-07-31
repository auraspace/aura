/** Production origin for canonical URLs and Open Graph. */
export const SITE_ORIGIN = 'https://aura.fadosoft.com'

export const SITE_NAME = 'Aura'

export const DEFAULT_TITLE = 'Aura · Easy to write. Boring to deploy.'

export const DEFAULT_DESCRIPTION =
  'Aura is a statically typed language with familiar classes, null-safe types, GC, and native builds in one Rust-powered toolchain.'

/**
 * Absolute public URL for a route path.
 * - Home is `${origin}/`
 * - Other routes have no trailing slash: `${origin}/docs/getting-started`
 * - Optional Vite/router base (e.g. `/repo`) is included when not `/`.
 */
export function absoluteUrl(pathname: string, base = '/'): string {
  const basePart = base === '/' ? '' : base.replace(/\/$/, '')
  let path = pathname.startsWith('/') ? pathname : `/${pathname}`
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1)
  if (path === '/') return `${SITE_ORIGIN}${basePart}/`
  return `${SITE_ORIGIN}${basePart}${path}`
}

export function escapeHtmlAttr(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}
