/**
 * Copy repo scripts/install.sh → public/install.sh so Vite emits
 * https://aura.fadosoft.com/install.sh from site/dist.
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const siteRoot = path.resolve(__dirname, '..')
const src = path.resolve(siteRoot, '../scripts/install.sh')
const dest = path.join(siteRoot, 'public/install.sh')

if (!fs.existsSync(src)) {
  console.error(`sync-install: missing ${src}`)
  process.exit(1)
}

fs.mkdirSync(path.dirname(dest), { recursive: true })
fs.copyFileSync(src, dest)
// Ensure executable bit is not required on the CDN; content is what matters.
console.log(`sync-install: ${path.relative(siteRoot, src)} → public/install.sh`)
