import { execFileSync } from 'node:child_process'
import { chmodSync, cpSync, existsSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const repositoryRoot = resolve(extensionRoot, '../../..')
const targetPlatform = process.env.AURA_TARGET_PLATFORM ?? process.platform
const targetArch = process.env.AURA_TARGET_ARCH ?? process.arch
const target = `${targetPlatform}-${targetArch}`
const executable = targetPlatform === 'win32' ? 'auralsp.exe' : 'auralsp'
const destination = join(extensionRoot, 'bin', target, executable)
const suppliedBinary = process.env.AURA_LSP_BINARY

mkdirSync(dirname(destination), { recursive: true })
if (suppliedBinary) {
  if (!existsSync(suppliedBinary)) {
    throw new Error(`AURA_LSP_BINARY does not exist: ${suppliedBinary}`)
  }
  cpSync(suppliedBinary, destination)
} else {
  if (targetPlatform !== process.platform || targetArch !== process.arch) {
    throw new Error(
      `Cross-target packaging requires AURA_LSP_BINARY for ${target}`,
    )
  }
  execFileSync('cargo', ['build', '--release', '-p', 'aura-lsp'], {
    cwd: repositoryRoot,
    stdio: 'inherit',
  })
  const builtBinary = join(repositoryRoot, 'target', 'release', executable)
  if (!existsSync(builtBinary)) {
    throw new Error(`cargo did not produce ${builtBinary}`)
  }
  cpSync(builtBinary, destination)
}

if (targetPlatform !== 'win32') {
  chmodSync(destination, 0o755)
}
console.log(`Staged embedded Aura LSP: ${destination}`)
