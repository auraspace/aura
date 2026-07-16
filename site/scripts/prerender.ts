/**
 * After `vite build`, prerender routes into dist/**\/index.html
 * using Vite SSR module loader (supports import.meta.glob).
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(__dirname, '..')
const dist = path.join(root, 'dist')

const baseEnv = process.env.VITE_BASE || '/'
/** Router basename: no trailing slash (e.g. `/aura`), or undefined at site root. */
const basename =
  baseEnv === '/' ? undefined : baseEnv.replace(/\/$/, '') || undefined

/** StaticRouter location must include basename when set (e.g. `/aura/rfc/000`). */
function locationFor(route: string): string {
  if (!basename) return route
  if (route === '/') return `${basename}/`
  return `${basename}${route}`
}

async function main() {
  const templatePath = path.join(dist, 'index.html')
  if (!fs.existsSync(templatePath)) {
    throw new Error(`Missing ${templatePath}. Run vite build first.`)
  }
  const template = fs.readFileSync(templatePath, 'utf8')

  const vite = await createServer({
    root,
    base: baseEnv,
    server: { middlewareMode: true },
    appType: 'custom',
  })

  try {
    const { render } = (await vite.ssrLoadModule(
      '/src/entry-server.tsx',
    )) as {
      render: (url: string, basename?: string) => string
    }
    const { getAllRfcs } = (await vite.ssrLoadModule(
      '/src/lib/rfc/load-rfcs.ts',
    )) as {
      getAllRfcs: () => { id: string }[]
    }

    const rfcs = getAllRfcs()
    // Feature routes under /rfc (add /docs, landing, … here later).
    const routes = [
      '/rfc',
      '/rfc/graph',
      ...rfcs.map((r) => `/rfc/${r.id}`),
    ]

    for (const route of routes) {
      const appHtml = render(locationFor(route), basename)
      if (!appHtml || appHtml.length < 20) {
        throw new Error(
          `Prerender produced empty HTML for ${route} (location=${locationFor(route)}, basename=${basename ?? '/'})`,
        )
      }
      const page = template.replace(
        '<div id="root"></div>',
        `<div id="root">${appHtml}</div>`,
      )

      const out =
        route === '/'
          ? path.join(dist, 'index.html')
          : path.join(dist, route.replace(/^\//, ''), 'index.html')

      fs.mkdirSync(path.dirname(out), { recursive: true })
      fs.writeFileSync(out, page)
      console.log('prerender', route, '→', path.relative(dist, out))
    }

    fs.copyFileSync(path.join(dist, 'index.html'), path.join(dist, '404.html'))
    console.log(`done ${routes.length} routes (+ 404.html)`)
  } finally {
    await vite.close()
  }
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
