import { spawn } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { build, context } from 'esbuild'

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const options = {
  absWorkingDir: extensionRoot,
  entryPoints: ['src/extension.ts'],
  bundle: true,
  external: ['vscode'],
  format: 'cjs',
  outfile: 'dist/extension.js',
  platform: 'node',
  sourcemap: true,
}

if (!process.argv.includes('--watch')) {
  await build(options)
  process.exit(0)
}

const bundleContext = await context(options)
await bundleContext.watch()

const require = createRequire(import.meta.url)
const tscPath = require.resolve('typescript/bin/tsc')
const typecheck = spawn(
  process.execPath,
  [tscPath, '-p', './tsconfig.json', '--watch', '--preserveWatchOutput'],
  { cwd: extensionRoot, stdio: 'inherit' },
)

async function shutdown(signal) {
  typecheck.kill(signal)
  await bundleContext.dispose()
}

process.on('SIGINT', () => void shutdown('SIGINT'))
process.on('SIGTERM', () => void shutdown('SIGTERM'))
typecheck.on('exit', async (code) => {
  await bundleContext.dispose()
  process.exit(code ?? 0)
})
