import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

export type ToolchainChoice = {
  label: string
  description: string
  detail: string
  command: string
  args: string[]
}

type DiscoveryDependencies = {
  findExecutable: (name: string) => Promise<string | undefined>
  listAvmVersions: () => Promise<string[]>
  auraHome: string
  extensionPath: string
  platform: NodeJS.Platform
}

function executableName(name: string, platform: NodeJS.Platform): string {
  return platform === 'win32' ? `${name}.exe` : name
}

function bundledLspPath(
  extensionPath: string,
  platform: NodeJS.Platform,
  arch: string,
): string {
  return join(
    extensionPath,
    'bin',
    `${platform}-${arch}`,
    executableName('auralsp', platform),
  )
}

async function findExecutable(name: string): Promise<string | undefined> {
  const command = process.platform === 'win32' ? 'where.exe' : 'which'
  try {
    const { stdout } = await execFileAsync(command, [name])
    return stdout
      .split(/\r?\n/)
      .find((line) => line.trim())
      ?.trim()
  } catch {
    return undefined
  }
}

export async function supportsLanguageServer(
  command: string,
): Promise<boolean> {
  try {
    const { stdout } = await execFileAsync(command, ['help'])
    return /\b(?:language-server|lsp)\b/.test(stdout)
  } catch {
    return false
  }
}

async function listAvmVersions(): Promise<string[]> {
  const avm = await findExecutable('avm')
  if (!avm) {
    return []
  }

  try {
    const { stdout } = await execFileAsync(avm, ['--list'])
    return stdout
      .split(/\r?\n/)
      .map((version) => version.trim())
      .filter(Boolean)
  } catch {
    return []
  }
}

function avmChoices(
  versions: string[],
  auraHome: string,
  platform: NodeJS.Platform,
): ToolchainChoice[] {
  const aura = executableName('aura', platform)
  return versions
    .map((version) => {
      const command = join(auraHome, 'versions', version, 'bin', aura)
      return { version, command }
    })
    .filter(({ command }) => existsSync(command))
    .map(({ version, command }) => ({
      label: `AVM: Aura ${version}`,
      description: 'Installed version manager toolchain',
      detail: command,
      command,
      args: ['language-server'],
    }))
}

export function isLanguageServerPath(command: string): boolean {
  return /(?:^|[\\/])auralsp(?:\.exe)?$/i.test(command.trim())
}

export function buildToolchainChoices({
  auraPath,
  lspPath,
  avmVersions,
  auraHome,
  extensionPath,
  platform,
  arch = process.arch,
}: {
  auraPath?: string
  lspPath?: string
  avmVersions: string[]
  auraHome: string
  extensionPath?: string
  platform: NodeJS.Platform
  arch?: string
}): ToolchainChoice[] {
  const choices: ToolchainChoice[] = []
  if (auraPath) {
    choices.push({
      label: 'Current Aura (PATH)',
      description: 'Use the current aura executable',
      detail: auraPath,
      command: auraPath,
      args: ['language-server'],
    })
  }
  if (lspPath) {
    choices.push({
      label: 'Aura LSP (PATH)',
      description: 'Run auralsp directly',
      detail: lspPath,
      command: lspPath,
      args: [],
    })
  }
  if (extensionPath) {
    const command = bundledLspPath(extensionPath, platform, arch)
    if (existsSync(command)) {
      choices.push({
        label: 'Bundled Aura LSP',
        description: 'Use the language server packaged with this extension',
        detail: command,
        command,
        args: [],
      })
    }
  }
  choices.push(...avmChoices(avmVersions, auraHome, platform))
  return choices
}

export async function discoverToolchains(
  dependencies: Partial<DiscoveryDependencies> = {},
): Promise<ToolchainChoice[]> {
  const platform = dependencies.platform ?? process.platform
  const auraHome =
    dependencies.auraHome ?? process.env.AURA_HOME ?? join(homedir(), '.aura')
  const lookup = dependencies.findExecutable ?? findExecutable
  const versions = dependencies.listAvmVersions ?? listAvmVersions
  const [auraCandidate, lspCandidate, avmVersions] = await Promise.all([
    lookup('aura'),
    lookup('auralsp'),
    versions(),
  ])
  const lspPath =
    lspCandidate && (await supportsLanguageServer(lspCandidate))
      ? lspCandidate
      : undefined
  const auraPath =
    auraCandidate && (await supportsLanguageServer(auraCandidate))
      ? auraCandidate
      : undefined
  return buildToolchainChoices({
    auraPath,
    lspPath,
    avmVersions,
    auraHome,
    extensionPath: dependencies.extensionPath,
    platform,
  })
}
