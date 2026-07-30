import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { promisify } from 'node:util'

import * as vscode from 'vscode'

const execFileAsync = promisify(execFile)

export type AuraCommandResult = {
  code: number
  stdout: string
  stderr: string
}

export function workspaceRoot(): string | undefined {
  const active = vscode.window.activeTextEditor?.document.uri
  const folder = active
    ? vscode.workspace.getWorkspaceFolder(active)
    : vscode.workspace.workspaceFolders?.[0]
  return folder?.uri.fsPath
}

export function projectArgument(root: string): string {
  return existsSync(join(root, 'aura.toml')) ? '.' : root
}

export async function runAuraCommand(
  command: string,
  args: string[],
  cwd: string,
): Promise<AuraCommandResult> {
  try {
    const result = await execFileAsync(command, args, {
      cwd,
      maxBuffer: 10 * 1024 * 1024,
      windowsHide: true,
    })
    return { code: 0, stdout: result.stdout, stderr: result.stderr }
  } catch (error) {
    const failure = error as {
      code?: number | string
      stdout?: string
      stderr?: string
      message?: string
    }
    return {
      code: typeof failure.code === 'number' ? failure.code : 1,
      stdout: failure.stdout ?? '',
      stderr: failure.stderr ?? failure.message ?? String(error),
    }
  }
}
