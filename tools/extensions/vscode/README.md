# Aura Language Support for VS Code

This extension connects VS Code to Aura's `auralsp` language server over stdio.
It provides syntax highlighting and editor behavior for Aura source, plus every
capability currently advertised by `auralsp`: diagnostics, completion, hover,
go to definition, references, rename, document and workspace symbols,
formatting, and quick-fix/source-format code actions. These features work for
saved `.aura` files and unsaved buffers whose language mode is Aura.

The Command Palette also provides `Aura: Check Project`, `Aura: Build
Project`, `Aura: Test Project`, `Aura: Race Tests`, and `Aura: Format Project`.
Their output is collected in the **Aura Language Server** output channel. The
status bar shows whether the server is starting, ready, or unavailable.

Syntax highlighting follows the compiler lexer and includes declarations,
attributes, strings and escapes, line/block comments, integer literals,
operators (including `?.`, `?:`, ranges, and lambda/type arrows), built-in
types, and the current language keyword set, including async task keywords.

## Requirements

Install the Aura CLI and make the `aura` executable available on `PATH`:

```sh
aura --version
```

The default server command is equivalent to `aura language-server`. For a
custom build, set `aura.serverPath` to the executable and replace
`aura.serverArgs` if needed.
Project commands use `aura.cliPath`, which defaults to `aura` and can be set
independently when the language server is a standalone `auralsp` binary.
Set `aura.trace.server` to `messages` or `verbose` when diagnosing an LSP
integration problem; leave it at `off` during normal use.

If `aura` is not available on `PATH`, the extension starts the embedded
`auralsp` binary for the current VS Code platform. Windows packages use
`bin/win32-x64/auralsp.exe` (or the matching VS Code architecture).

## Select a toolchain

Run **Aura: Select Toolchain** from the Command Palette to detect and choose:

- the current `aura` executable on `PATH`;
- a standalone `auralsp` executable on `PATH`;
- the `auralsp` binary bundled in the extension, when available;
- every valid version reported by `avm --list`; or
- a custom path to either executable.

The selection updates `aura.serverPath` and `aura.serverArgs` for the
current workspace (or globally when no workspace is open), then restarts the
language server. AVM versions are resolved from `$AURA_HOME/versions/<version>/bin/aura`.

## Development

```sh
npm install
npm test
npm run check
npm run compile
npm run package
```

For a Windows or another cross-target VSIX, provide a target-built binary:

```powershell
$env:AURA_TARGET_PLATFORM = "win32"
$env:AURA_TARGET_ARCH = "x64"
$env:AURA_LSP_BINARY = "C:\path\to\auralsp.exe"
npm run package
```

Press `F5` from this directory in VS Code to launch an Extension Development
Host. Use `Aura: Restart Language Server` after changing the toolchain path.

When the repository root is open in VS Code, use the **Run Aura VS Code
Extension** launch configuration. The workspace also provides tasks to watch,
test, and package the extension, plus build or test `aura-lsp`.

The extension uses LSP capabilities negotiated with the bundled or installed
server, so unsupported future server features remain disabled until `auralsp`
advertises them.
