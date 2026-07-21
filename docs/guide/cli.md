---
title: CLI
section: Toolchain
order: 40
summary: aura new, init, version, check, build, run, and test — the verbs you use every day.
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

## Commands (0.1.0-alpha)

| Command              | Purpose                                   |
| -------------------- | ----------------------------------------- |
| `new <path>`         | Scaffold a package directory              |
| `init [name]`        | Scaffold in the current directory         |
| `check <file\|dir>`  | Parse + typecheck                         |
| `build <file\|dir>`  | Emit native binary (`-o` for output path) |
| `run <file\|dir>`    | Build and execute                         |
| `test <file\|dir>`   | Run `@test` functions                     |
| `emit-c <file\|dir>` | Emit C (advanced / debugging)             |
| `version`            | Print CLI version (`aura 0.1.0-alpha`)    |

Examples:

```bash
aura new hello
aura run hello
aura check path
aura build path -o out
aura test path
aura version
```

Monorepo corpus smokes:

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/test/smoke.aura
cargo run -p aura-cli -- build corpus/hello/main.aura -o target/aura/hello
```

## Inputs

- A **single `.aura` file**, or
- A **package directory** containing `aura.toml` and `src/` (or `aura.toml` path)

With no path, package commands look for `./aura.toml`. Package mode unlocks multi-file compilation, imports, and path dependencies. See [Packages](./packages.md).

## Runtime and linking

`build` / `run` use the **C backend**: Aura → C → system `cc`, linked with `aura_rt.c` (embedded in the CLI, or from the release tree / `AURA_RUNTIME`). LLVM IR remains the longer-term backend ([RFC-004](/rfc/004)).

## Diagnostics

Type and name errors print human-readable messages (`path:line:col` + snippet). Prefer `check` in editors/CI when you only need validation.

## Scaffolding

```bash
aura new my_app          # creates my_app/aura.toml + my_app/src/main.aura
aura init                # same layout in `.` (name from directory)
```

Hyphens in the path become underscores in the package name (`my-app` → package `my_app`). Existing `aura.toml` / `src/` are never overwritten.

## Not in alpha

RFC-012 also describes `fmt`, package registry flows, and `doc`. Those are **not** implemented yet. Also out of scope for this release: program **argv** / stdin line reader (demos use fixed paths).

## Next

- [Packages](./packages.md)
- [Testing](./testing.md)
- [Install](./install.md)
