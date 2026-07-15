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
const basename =
  baseEnv === '/' ? undefined : baseEnv.replace(/\/$/, '') || undefined

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
      '/src/lib/load-rfcs.ts',
    )) as {
      getAllRfcs: () => { id: string }[]
    }

    const rfcs = getAllRfcs()
    const routes = ['/', '/graph', ...rfcs.map((r) => `/rfc/${r.id}`)]

    for (const route of routes) {
      const appHtml = render(route, basename)
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
