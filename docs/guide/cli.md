---
title: CLI
section: Toolchain
order: 40
summary: aura new, init, check, build, run, test, fmt, race, update, and language-server.
---

# CLI

The `aura` CLI is the day-to-day surface of the toolchain ([RFC-012](/rfc/012)).

After [install](./install.md):

```bash
aura <command> [args]
```

From this monorepo without a global install:

```bash
cargo run -p aura-cli -- <command> [args]
```

## Commands (0.1.1-alpha.7)

| Command                   | Purpose                                        |
| ------------------------- | ---------------------------------------------- |
| `new <path>`              | Scaffold a package directory                   |
| `init [name]`             | Scaffold in the current directory              |
| `check <file\|dir>`       | Parse + typecheck                              |
| `build <file\|dir>`       | Emit native binary (`-o` for output path)      |
| `run <file\|dir>`         | Build and execute                              |
| `test <file\|dir>`        | Run `@test` functions                          |
| `race <file\|dir>`        | Run tests with the runtime race detector       |
| `fmt [--check] <path>`    | Format/check a source file, package, or folder |
| `lsp` / `language-server` | Run the stdio Aura language server (`auralsp`) |
| `emit-c <file\|dir>`      | Emit C (advanced / debugging)                  |
| `update`                  | Check, or activate, a toolchain update         |
| `version`                 | Print the installed CLI version                |

Examples:

```bash
aura new hello
aura run hello
aura check path
aura build path -o out
aura test path
aura test path --test-name filter --format json
aura race path --format json
aura fmt path/src/main.aura
aura update --package aura --current 0.1.1-alpha.7
auralsp
# or: aura lsp
aura version
```

### Process arguments after `--` (C12c)

`aura run` and `aura test` forward everything after `--` to the process. Programs read them with `std.io.args()` (C12b): index `0` is the program name; user flags start at index `1`.

```bash
aura run examples/wc -- target/aura/wc_sample.txt
aura run examples/wc -- -lwc -n 1 path/to/file
cargo run -p aura-cli -- run corpus/std_io/args -- hello
```

Inside Aura:

```aura
val argv = args()          // Array<String>
// argv[0] = program path; argv[1]… = flags after --
```

See [Standard library](./standard-library.md#stdio) and dogfood `examples/wc`.

Monorepo corpus smokes:

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/test/smoke.aura
cargo run -p aura-cli -- build corpus/hello/main.aura -o target/aura/hello
cargo run -p aura-cli -- run corpus/std_io/args -- hello
cargo run -p aura-cli -- run examples/wc -- path/to/file
```

## Inputs

- A **single `.aura` file**, or
- A **package directory** containing `aura.toml` and `src/` (or `aura.toml` path)

With no path, package commands look for `./aura.toml`. Package mode unlocks multi-file compilation, imports, and path dependencies. See [Packages](./packages.md).

## Runtime and linking

`build` / `run` use the **C backend** by default: Aura → C → system `cc`. The compatibility source `runtime.c` is embedded in the CLI; set `AURA_RUNTIME_LIB` to link the prebuilt `libaurart.a` archive instead. Use `aura emit-llvm` to inspect LLVM IR or `aura build --backend llvm` for complete MIR programs; set `AURA_LLVM_RUNTIME_LIB` to link `libaurart-llvm.a`. LLVM builds require `clang`; unsupported MIR operations are rejected before an artifact is created.

The LLVM implementation is organized under `crates/aura-codegen/src/backends/llvm/`: the facade, options, MIR emitter, native compiler adapter, and tests are separate modules.

`build` accepts one input path and optionally `-o <binary>`. `run` builds into
`target/aura/` and forwards arguments after `--`. `test` accepts
`--test-name <pattern>` (alias `--filter`) and `--format json`; `race` is the
test command with detector mode enabled.

## Diagnostics

Type and name errors print human-readable messages (`path:line:col` + snippet). Prefer `check` in editors/CI when you only need validation.

## Scaffolding

```bash
aura new my_app          # creates my_app/aura.toml + my_app/src/main.aura
aura init                # same layout in `.` (name from directory)
```

Hyphens in the path become underscores in the package name (`my-app` → package `my_app`). Existing `aura.toml` / `src/` are never overwritten.

## Registry and update commands

Package publication is an ordinary Git operation: commit the package, create
and push an immutable `vX.Y.Z` origin tag, and let consumers discover that tag.
GitHub Releases are optional for packages. A proxy and checksum database are
intentionally deferred. `update` checks a toolchain registry; `--activate`
downloads and atomically switches the active executable, with `--json` for
machine-readable output. `add` and `remove` update `[dependencies]` and
refresh `aura.lock`; `add` accepts a full VCS origin or `owner/repo` GitHub
shorthand, with optional `@version` and `--subdir`.

RFC-012 also describes `doc`, `clean`, and a complete toolchain manager; those
remain deferred. Process argv, stdin (`readLine` / `readAllStdin`), and
`exit` are available via `std.io`.

## Next

- [Packages](./packages.md)
- [Standard library](./standard-library.md)
- [Testing](./testing.md)
- [Install](./install.md)
