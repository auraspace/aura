import * as vscode from 'vscode'
import { Trace } from 'vscode-jsonrpc'
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node'

import { projectArgument, runAuraCommand, workspaceRoot } from './cli'
import { selectServer } from './server'
import {
  discoverToolchains,
  isLanguageServerPath,
  ToolchainChoice,
} from './toolchain'

let client: LanguageClient | undefined
let restartTimer: ReturnType<typeof setTimeout> | undefined
let status: vscode.StatusBarItem | undefined

function traceLevel(): Trace {
  const value = vscode.workspace
    .getConfiguration('aura')
    .get<string>('trace.server', 'off')
  if (value === 'verbose') return Trace.Verbose
  if (value === 'messages') return Trace.Messages
  return Trace.Off
}

function setStatus(text: string, tooltip?: string): void {
  if (!status) return
  status.text = text
  status.tooltip = tooltip ?? text
}

function serverConfiguration(): {
  configuredPath: string
  configuredArgs: string[]
} {
  const configuration = vscode.workspace.getConfiguration('aura')
  const configuredPath = configuration.get<string>('serverPath', 'aura').trim()
  const configuredArgs = configuration.get<unknown[]>('serverArgs', [
    'language-server',
  ])
  const args = Array.isArray(configuredArgs)
    ? configuredArgs.filter((arg): arg is string => typeof arg === 'string')
    : ['language-server']

  return {
    configuredPath: configuredPath || 'aura',
    configuredArgs: args,
  }
}

function createClient(
  output: vscode.OutputChannel,
  extensionPath: string,
): LanguageClient {
  const configuration = serverConfiguration()
  const server = selectServer({ ...configuration, extensionPath })
  output.appendLine(`Starting ${server.source} server: ${server.command}`)
  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath
  const options = cwd ? { cwd } : undefined
  const serverOptions: ServerOptions = {
    run: { command: server.command, args: server.args, options },
    debug: { command: server.command, args: server.args, options },
  }
  const clientOptions: LanguageClientOptions = {
    // Include untitled Aura buffers so language features work before first save.
    documentSelector: [{ language: 'aura' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.aura'),
    },
    outputChannel: output,
    traceOutputChannel: output,
  }

  const languageClient = new LanguageClient(
    'auraLanguageServer',
    'Aura Language Server',
    serverOptions,
    clientOptions,
  )
  void languageClient.setTrace(traceLevel())
  return languageClient
}

async function restartClient(
  output: vscode.OutputChannel,
  extensionPath: string,
): Promise<void> {
  if (client) {
    try {
      await client.stop()
    } catch (error) {
      output.appendLine(
        `Previous Aura language client did not stop cleanly: ${String(error)}`,
      )
    }
  }
  setStatus('$(sync~spin) Aura: starting')
  client = createClient(output, extensionPath)
  try {
    await client.start()
    setStatus('$(check) Aura: ready', 'Aura language server is running')
  } catch (error) {
    setStatus('$(error) Aura: unavailable', String(error))
    output.appendLine(`Failed to start Aura language server: ${String(error)}`)
    void vscode.window.showErrorMessage(
      'Aura language server did not start. See the Aura Language Server output.',
    )
  }
}

async function runProjectCommand(
  output: vscode.OutputChannel,
  command: string,
  args: string[] = [],
): Promise<void> {
  const root = workspaceRoot()
  if (!root) {
    void vscode.window.showWarningMessage('Open an Aura workspace first.')
    return
  }
  const cli =
    vscode.workspace
      .getConfiguration('aura')
      .get<string>('cliPath', 'aura')
      .trim() || 'aura'
  const fullArgs = [command, ...args, projectArgument(root)]
  output.appendLine(`$ ${cli} ${fullArgs.join(' ')}`)
  setStatus(`$(sync~spin) Aura: ${command}`)
  const result = await runAuraCommand(cli, fullArgs, root)
  if (result.stdout) output.append(result.stdout)
  if (result.stderr) output.append(result.stderr)
  if (result.code === 0) {
    setStatus('$(check) Aura: ready', 'Aura command completed successfully')
    void vscode.window.showInformationMessage(`Aura ${command} completed.`)
  } else {
    setStatus(
      '$(error) Aura: command failed',
      `Aura ${command} exited with code ${result.code}`,
    )
    void vscode.window.showErrorMessage(
      `Aura ${command} failed. See the Aura Language Server output.`,
    )
  }
  output.show(true)
}

function scheduleRestart(
  output: vscode.OutputChannel,
  extensionPath: string,
): void {
  if (restartTimer) {
    clearTimeout(restartTimer)
  }
  restartTimer = setTimeout(() => {
    restartTimer = undefined
    void restartClient(output, extensionPath)
  }, 0)
}

async function selectToolchain(
  output: vscode.OutputChannel,
  extensionPath: string,
): Promise<void> {
  const detected = await discoverToolchains({ extensionPath })
  const custom: vscode.QuickPickItem = {
    label: 'Custom path...',
    description: 'Choose an Aura CLI or auralsp executable',
  }
  const choice = await vscode.window.showQuickPick([...detected, custom], {
    placeHolder: 'Select the Aura toolchain used for language features',
  })
  if (!choice) {
    return
  }

  let toolchain: ToolchainChoice
  if (!('command' in choice)) {
    const command = await vscode.window.showInputBox({
      prompt: 'Path to an Aura CLI or auralsp executable',
      ignoreFocusOut: true,
    })
    if (!command?.trim()) {
      return
    }
    toolchain = {
      label: 'Custom Aura toolchain',
      description: 'Custom executable',
      detail: command.trim(),
      command: command.trim(),
      args: isLanguageServerPath(command) ? [] : ['language-server'],
    }
  } else {
    toolchain = choice
  }

  const target = vscode.workspace.workspaceFolders
    ? vscode.ConfigurationTarget.Workspace
    : vscode.ConfigurationTarget.Global
  const configuration = vscode.workspace.getConfiguration('aura')
  await configuration.update('serverPath', toolchain.command, target)
  await configuration.update('serverArgs', toolchain.args, target)
  output.appendLine(`Selected Aura toolchain: ${toolchain.command}`)
  void vscode.window.showInformationMessage(
    `Aura language server now uses ${toolchain.label}.`,
  )
}

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  const output = vscode.window.createOutputChannel('Aura Language Server')
  context.subscriptions.push(output)
  status = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100,
  )
  status.command = 'aura.selectToolchain'
  status.show()
  context.subscriptions.push(status)

  const restart = vscode.commands.registerCommand(
    'aura.restartLanguageServer',
    async () => {
      await restartClient(output, context.extensionPath)
    },
  )
  context.subscriptions.push(restart)

  const select = vscode.commands.registerCommand(
    'aura.selectToolchain',
    async () => selectToolchain(output, context.extensionPath),
  )
  context.subscriptions.push(select)

  for (const [command, cliCommand] of [
    ['aura.checkProject', 'check'],
    ['aura.buildProject', 'build'],
    ['aura.testProject', 'test'],
    ['aura.raceProject', 'race'],
    ['aura.formatProject', 'fmt'],
  ] as const) {
    context.subscriptions.push(
      vscode.commands.registerCommand(command, () =>
        runProjectCommand(output, cliCommand),
      ),
    )
  }
  context.subscriptions.push(
    vscode.commands.registerCommand('aura.showOutput', () => output.show()),
  )

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (
        event.affectsConfiguration('aura.serverPath') ||
        event.affectsConfiguration('aura.serverArgs') ||
        event.affectsConfiguration('aura.trace.server')
      ) {
        scheduleRestart(output, context.extensionPath)
      }
    }),
  )

  await restartClient(output, context.extensionPath)
}

export async function deactivate(): Promise<void> {
  if (restartTimer) {
    clearTimeout(restartTimer)
    restartTimer = undefined
  }
  await client?.stop()
  client = undefined
  status = undefined
}
