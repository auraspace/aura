/**
 * Build public/install.sh for the CDN:
 *   scripts/install.sh  +  embed(scripts/avm)  →  site/public/install.sh
 *
 * Vite then emits https://aura.fadosoft.com/install.sh from site/dist.
 *
 * Source of truth:
 *   - scripts/avm         Aura Version Manager helper (plain bash)
 *   - scripts/install.sh  installer template (AVM_SCRIPT_B64 empty)
 *
 * Why embed: curl|bash cannot read sibling files; large heredocs inside bash
 * functions hang on some builds, so we inject base64 at build time only.
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const siteRoot = path.resolve(__dirname, '..')
const repoRoot = path.resolve(siteRoot, '..')
const installSrc = path.join(repoRoot, 'scripts/install.sh')
const avmSrc = path.join(repoRoot, 'scripts/avm')
const dest = path.join(siteRoot, 'public/install.sh')

const BEGIN = '# @AVM_EMBED_BEGIN@'
const END = '# @AVM_EMBED_END@'

function die(msg) {
  console.error(`sync-install: ${msg}`)
  process.exit(1)
}

if (!fs.existsSync(installSrc)) die(`missing ${installSrc}`)
if (!fs.existsSync(avmSrc)) die(`missing ${avmSrc}`)

const avmBytes = fs.readFileSync(avmSrc)
if (avmBytes.length === 0) die('scripts/avm is empty')
// Reject CR-only weirdness; allow optional final newline.
const avmText = avmBytes.toString('utf8')
if (!avmText.startsWith('#!')) {
  die('scripts/avm must start with a shebang')
}
const b64 = avmBytes.toString('base64')

let install = fs.readFileSync(installSrc, 'utf8')
const beginAt = install.indexOf(BEGIN)
const endAt = install.indexOf(END)
if (beginAt < 0 || endAt < 0 || endAt < beginAt) {
  die(`install.sh missing embed markers ${BEGIN} … ${END}`)
}

const before = install.slice(0, beginAt)
const after = install.slice(endAt + END.length)
// Keep trailing newline after END if present in original
const afterNorm = after.startsWith('\n') ? after : `\n${after}`

const embedded = [
  BEGIN,
  // Single-quoted bash string: base64 alphabet is safe (A–Za–z0–9+/=).
  `AVM_SCRIPT_B64='${b64}'`,
  END,
].join('\n')

install = `${before}${embedded}${afterNorm}`

// Sanity: must not leave empty embed for CDN artifact.
if (!/AVM_SCRIPT_B64='[A-Za-z0-9+/=]+'/.test(install)) {
  die('failed to embed AVM_SCRIPT_B64')
}

fs.mkdirSync(path.dirname(dest), { recursive: true })
fs.writeFileSync(dest, install, 'utf8')

const rel = (p) => path.relative(siteRoot, p)
console.log(
  `sync-install: ${rel(installSrc)} + ${rel(avmSrc)} → ${rel(dest)} (avm ${avmBytes.length}B → b64 ${b64.length} chars)`,
)
