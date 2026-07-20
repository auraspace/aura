import {
  absoluteUrl,
  DEFAULT_DESCRIPTION,
  DEFAULT_TITLE,
  escapeHtmlAttr,
  SITE_NAME,
} from './site'

export type PageMeta = {
  /** Full document title */
  title: string
  description: string
  /** App route path, e.g. `/docs/getting-started` */
  path: string
}

export type PageMetaContext = {
  guides: { slug: string; title: string; summary: string }[]
  rfcs: { id: string; title: string }[]
}

const STATIC: Record<string, { title: string; description: string }> = {
  '/': {
    title: DEFAULT_TITLE,
    description: DEFAULT_DESCRIPTION,
  },
  '/docs': {
    title: `Documentation · ${SITE_NAME}`,
    description:
      'Practical guides for learning and using Aura — syntax, types, packages, CLI, and more.',
  },
  '/rfc': {
    title: `RFC catalog · ${SITE_NAME}`,
    description:
      'Design decisions and contracts for the Aura language, toolchain, and standard library.',
  },
  '/rfc/graph': {
    title: `RFC dependency graph · ${SITE_NAME}`,
    description:
      'How Aura RFCs depend on and block each other across the design stack.',
  },
}

/** Resolve title/description for a route (no trailing slash except `/`). */
export function pageMetaForRoute(
  route: string,
  ctx: PageMetaContext = { guides: [], rfcs: [] },
): PageMeta {
  let path = route.startsWith('/') ? route : `/${route}`
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1)

  const staticMeta = STATIC[path]
  if (staticMeta) {
    return {
      path,
      title: staticMeta.title,
      description: staticMeta.description,
    }
  }

  const docsMatch = path.match(/^\/docs\/([^/]+)$/)
  if (docsMatch) {
    const guide = ctx.guides.find((g) => g.slug === docsMatch[1])
    if (guide) {
      return {
        path,
        title: `${guide.title} · Docs · ${SITE_NAME}`,
        description:
          guide.summary?.trim() ||
          `${guide.title} — Aura language documentation.`,
      }
    }
  }

  const rfcMatch = path.match(/^\/rfc\/(\d+)$/)
  if (rfcMatch) {
    const id = rfcMatch[1].padStart(3, '0')
    const rfc = ctx.rfcs.find((r) => r.id === id || r.id === rfcMatch[1])
    if (rfc) {
      return {
        path: `/rfc/${id}`,
        title: `RFC-${id}: ${rfc.title} · ${SITE_NAME}`,
        description: `RFC-${id} — ${rfc.title}. Aura language design document.`,
      }
    }
  }

  return {
    path,
    title: `Not found · ${SITE_NAME}`,
    description: DEFAULT_DESCRIPTION,
  }
}

/** Build `<link rel="canonical">` + Open Graph / Twitter tags for a page. */
export function seoHeadHtml(meta: PageMeta, base = '/'): string {
  const url = escapeHtmlAttr(absoluteUrl(meta.path, base))
  const title = escapeHtmlAttr(meta.title)
  const description = escapeHtmlAttr(meta.description)
  const siteName = escapeHtmlAttr(SITE_NAME)

  return [
    `<link rel="canonical" href="${url}" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:site_name" content="${siteName}" />`,
    `<meta property="og:title" content="${title}" />`,
    `<meta property="og:description" content="${description}" />`,
    `<meta property="og:url" content="${url}" />`,
    `<meta name="twitter:card" content="summary" />`,
    `<meta name="twitter:title" content="${title}" />`,
    `<meta name="twitter:description" content="${description}" />`,
  ].join('\n    ')
}

/**
 * Inject title, description, and SEO tags into a Vite-built HTML shell.
 * Safe to call once per prerendered page.
 */
export function applyPageMetaToHtml(
  html: string,
  meta: PageMeta,
  base = '/',
): string {
  const title = escapeHtmlAttr(meta.title)
  const description = escapeHtmlAttr(meta.description)
  const headTags = seoHeadHtml(meta, base)

  let out = html.replace(/<title>[\s\S]*?<\/title>/, `<title>${title}</title>`)
  out = out.replace(
    /<meta\s+name="description"\s+content="[^"]*"\s*\/?>/s,
    `<meta name="description" content="${description}" />`,
  )
  // Drop a previous injection if present (idempotent).
  out = out.replace(/\s*<!--seo:start-->[\s\S]*?<!--seo:end-->/, '')
  out = out.replace(
    '</head>',
    `    <!--seo:start-->\n    ${headTags}\n    <!--seo:end-->\n  </head>`,
  )
  return out
}
