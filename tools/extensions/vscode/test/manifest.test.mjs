import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repositoryRoot = path.resolve(root, '../../..')
const manifest = JSON.parse(
  fs.readFileSync(path.join(root, 'package.json'), 'utf8'),
)

assert.equal(manifest.main, './dist/extension.js')
const bundledExtension = fs.readFileSync(path.join(root, manifest.main), 'utf8')
assert.doesNotMatch(
  bundledExtension,
  /require\(["']vscode-languageclient(?:\/node)?["']\)/,
)
assert.match(bundledExtension, /Aura Language Server/)
assert.ok(manifest.activationEvents.includes('onLanguage:aura'))
for (const command of [
  'aura.restartLanguageServer',
  'aura.selectToolchain',
  'aura.checkProject',
  'aura.buildProject',
  'aura.testProject',
  'aura.raceProject',
  'aura.formatProject',
  'aura.showOutput',
]) {
  assert.ok(manifest.activationEvents.includes(`onCommand:${command}`))
  assert.ok(
    manifest.contributes.commands.some(({ command: id }) => id === command),
  )
}
assert.equal(manifest.contributes.languages[0].id, 'aura')
assert.deepEqual(manifest.contributes.languages[0].extensions, ['.aura'])
assert.deepEqual(manifest.contributes.languages[0].icon, {
  light: './assets/aura-language-light.svg',
  dark: './assets/aura-language-dark.svg',
})
assert.equal(manifest.icon, 'icon.png')
assert.ok(fs.existsSync(path.join(root, manifest.icon)))
for (const icon of Object.values(manifest.contributes.languages[0].icon)) {
  assert.ok(fs.existsSync(path.join(root, icon)))
}
assert.equal(
  manifest.contributes.configuration.properties['aura.serverPath'].default,
  'aura',
)
assert.deepEqual(
  manifest.contributes.configuration.properties['aura.serverArgs'].default,
  ['language-server'],
)
assert.equal(
  manifest.contributes.configuration.properties['aura.cliPath'].default,
  'aura',
)
assert.deepEqual(
  manifest.contributes.configuration.properties['aura.trace.server'].enum,
  ['off', 'messages', 'verbose'],
)
assert.ok(
  manifest.contributes.commands.some(
    ({ command }) => command === 'aura.restartLanguageServer',
  ),
)
assert.ok(
  manifest.contributes.commands.some(
    ({ command }) => command === 'aura.selectToolchain',
  ),
)
const languageConfiguration = JSON.parse(
  fs.readFileSync(path.join(root, 'language-configuration.json'), 'utf8'),
)
const auraWordPattern = new RegExp(languageConfiguration.wordPattern, 'g')
assert.deepEqual('    await funcA()'.match(auraWordPattern), ['await', 'funcA'])
assert.deepEqual('box.value'.match(auraWordPattern), ['box', 'value'])
assert.equal(manifest.contributes.grammars[0].language, 'aura')
assert.equal(manifest.contributes.grammars[0].scopeName, 'source.aura')
const grammarPath = path.join(root, manifest.contributes.grammars[0].path)
const grammar = JSON.parse(fs.readFileSync(grammarPath, 'utf8'))
assert.equal(grammar.scopeName, 'source.aura')
assert.match(grammar.repository.keywords.match, /async/)
assert.match(grammar.repository.keywords.match, /extern/)
assert.match(grammar.repository.keywords.match, /interface/)
assert.ok(grammar.repository.operators.match.includes('\\?\\.'))
assert.match(grammar.repository.types.match, /TaskHandle/)
assert.match(grammar.repository.types.match, /ForeignHandle/)
assert.equal(grammar.repository['custom-types'].name, 'entity.name.type.aura')
assert.match('Notebook', new RegExp(grammar.repository['custom-types'].match))
assert.equal(
  grammar.repository.functions.patterns[0].captures['2'].name,
  'entity.name.function.aura',
)
assert.match(grammar.repository['comment-tasks'].patterns[0].match, /TODO/)
for (const keyword of [
  'abstract',
  'as',
  'async',
  'await',
  'break',
  'cancel',
  'case',
  'catch',
  'class',
  'companion',
  'const',
  'continue',
  'else',
  'enum',
  'extern',
  'false',
  'final',
  'finally',
  'for',
  'fun',
  'if',
  'import',
  'in',
  'interface',
  'is',
  'join',
  'match',
  'null',
  'object',
  'open',
  'override',
  'package',
  'private',
  'protected',
  'pub',
  'return',
  'spawn',
  'struct',
  'this',
  'throw',
  'true',
  'try',
  'type',
  'val',
  'var',
  'vararg',
  'where',
  'while',
]) {
  assert.match(keyword, new RegExp(grammar.repository.keywords.match))
}
const extensionSource = fs.readFileSync(
  path.join(root, 'src', 'extension.ts'),
  'utf8',
)
assert.match(extensionSource, /\{ language: 'aura', scheme: 'file' \}/)
assert.match(extensionSource, /\{ language: 'aura', scheme: 'untitled' \}/)
assert.equal(manifest.scripts['stage-server'], 'node scripts/stage-server.mjs')
assert.equal(manifest.scripts.bundle, 'node scripts/build-extension.mjs')
const buildScript = fs.readFileSync(
  path.join(root, 'scripts', 'build-extension.mjs'),
  'utf8',
)
assert.match(buildScript, /bundle: true/)
assert.match(buildScript, /external: \['vscode'\]/)

const tasks = JSON.parse(
  fs.readFileSync(path.join(repositoryRoot, '.vscode', 'tasks.json'), 'utf8'),
)
assert.equal(tasks.version, '2.0.0')
assert.ok(tasks.tasks.some(({ label }) => label === 'Aura Extension: Watch'))
assert.ok(tasks.tasks.some(({ label }) => label === 'Aura Extension: Test'))
assert.ok(tasks.tasks.some(({ label }) => label === 'Aura LSP: Test'))
const launch = JSON.parse(
  fs.readFileSync(path.join(repositoryRoot, '.vscode', 'launch.json'), 'utf8'),
)
assert.equal(launch.configurations[0].type, 'extensionHost')
assert.equal(launch.configurations[0].preLaunchTask, 'Aura Extension: Watch')

const server = await import('../out/server.js')
const oldAura = path.join(
  fs.mkdtempSync(path.join(os.tmpdir(), 'aura-old-')),
  'aura',
)
fs.writeFileSync(oldAura, '#!/bin/sh\nprintf "Aura toolchain\\n"\n')
fs.chmodSync(oldAura, 0o755)
assert.equal(server.supportsLanguageServer(oldAura), false)
const lspCapableAura = path.join(
  fs.mkdtempSync(path.join(os.tmpdir(), 'aura-lsp-capable-')),
  'aura',
)
fs.writeFileSync(
  lspCapableAura,
  '#!/bin/sh\nprintf "aura language-server\\n" >&2\n',
)
fs.chmodSync(lspCapableAura, 0o755)
assert.equal(server.supportsLanguageServer(lspCapableAura), true)
const toolchain = await import('../out/toolchain.js')
const avmHome = fs.mkdtempSync(path.join(os.tmpdir(), 'aura-avm-'))
const avmAura = path.join(avmHome, 'versions', '0.1.1-alpha.1', 'bin', 'aura')
fs.mkdirSync(path.dirname(avmAura), { recursive: true })
fs.writeFileSync(avmAura, 'test binary')
fs.chmodSync(avmAura, 0o755)
const bundledRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'aura-bundled-'))
const bundledLsp = path.join(bundledRoot, 'bin', 'darwin-arm64', 'auralsp')
fs.mkdirSync(path.dirname(bundledLsp), { recursive: true })
fs.writeFileSync(bundledLsp, 'test binary')
fs.chmodSync(bundledLsp, 0o755)
const choices = toolchain.buildToolchainChoices({
  auraPath: '/usr/local/bin/aura',
  lspPath: '/usr/local/bin/auralsp',
  avmVersions: ['0.1.1-alpha.1'],
  auraHome: avmHome,
  extensionPath: bundledRoot,
  platform: 'darwin',
  arch: 'arm64',
})
assert.deepEqual(
  choices.map(({ label, args }) => ({ label, args })),
  [
    { label: 'Current Aura (PATH)', args: ['language-server'] },
    { label: 'Aura LSP (PATH)', args: [] },
    { label: 'Bundled Aura LSP', args: [] },
    { label: 'AVM: Aura 0.1.1-alpha.1', args: ['language-server'] },
  ],
)
assert.equal(toolchain.isLanguageServerPath('/opt/aura/auralsp'), true)
assert.equal(toolchain.isLanguageServerPath('/opt/aura/aura'), false)
assert.equal(await toolchain.supportsLanguageServer(lspCapableAura), true)
assert.equal(await toolchain.supportsLanguageServer(oldAura), false)
const fakeExtension = fs.mkdtempSync(path.join(os.tmpdir(), 'aura-vscode-'))
const embedded = path.join(fakeExtension, 'bin', 'win32-x64', 'auralsp.exe')
fs.mkdirSync(path.dirname(embedded), { recursive: true })
fs.writeFileSync(embedded, 'test binary')
const selected = server.selectServer({
  configuredPath: 'aura',
  configuredArgs: ['language-server'],
  extensionPath: fakeExtension,
  platform: 'win32',
  arch: 'x64',
})
assert.equal(selected.source, 'embedded')
assert.equal(selected.command, embedded)
assert.deepEqual(selected.args, [])

console.log('VS Code extension manifest is valid')
