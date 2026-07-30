import { execFileSync, spawnSync } from 'node:child_process'
import { chmodSync, existsSync } from 'node:fs'
import { join } from 'node:path'

export type ServerConfiguration = {
  command: string
  args: string[]
  source: 'host' | 'embedded' | 'configured'
}

type ServerSelectionOptions = {
  configuredPath: string
  configuredArgs: string[]
  extensionPath: string
  platform?: NodeJS.Platform
  arch?: string
}

function hostAuraPath(platform: NodeJS.Platform): string | undefined {
  const lookup = platform === 'win32' ? 'where.exe' : 'which'
  try {
    const result = execFileSync(lookup, ['aura'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    })
    return result
      .split(/\r?\n/)
      .find((line) => line.trim())
      ?.trim()
  } catch {
    return undefined
  }
}

export function supportsLanguageServer(command: string): boolean {
  const result = spawnSync(command, ['help'], { encoding: 'utf8' })
  if (result.error) {
    return false
  }
  return /\b(?:language-server|lsp)\b/.test(
    `${result.stdout ?? ''}\n${result.stderr ?? ''}`,
  )
}

function embeddedServerPath(
  extensionPath: string,
  platform: NodeJS.Platform,
  arch: string,
): string {
  const target = `${platform}-${arch}`
  const executable = platform === 'win32' ? 'auralsp.exe' : 'auralsp'
  return join(extensionPath, 'bin', target, executable)
}

export function selectServer({
  configuredPath,
  configuredArgs,
  extensionPath,
  platform = process.platform,
  arch = process.arch,
}: ServerSelectionOptions): ServerConfiguration {
  const path = configuredPath.trim()
  if (path && path !== 'aura') {
    if (!isAuraCli(path) || supportsLanguageServer(path)) {
      return { command: path, args: configuredArgs, source: 'configured' }
    }
  }

  const hostPath = hostAuraPath(platform)
  if (hostPath && supportsLanguageServer(hostPath)) {
    return { command: hostPath, args: configuredArgs, source: 'host' }
  }

  const embeddedPath = embeddedServerPath(extensionPath, platform, arch)
  if (existsSync(embeddedPath)) {
    if (platform !== 'win32') {
      try {
        chmodSync(embeddedPath, 0o755)
      } catch {
        // The process may already have executable permissions.
      }
    }
    return { command: embeddedPath, args: [], source: 'embedded' }
  }

  // Preserve the useful process error when no matching embedded binary exists.
  return { command: path || 'aura', args: configuredArgs, source: 'host' }
}

function isAuraCli(command: string): boolean {
  return /(?:^|[\\/])aura(?:\.exe)?$/i.test(command)
}
