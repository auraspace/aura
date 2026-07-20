---
title: CLI
section: Toolchain
order: 40
summary: aura check, build, run, and test — the verbs you use every day.
---

# CLI

The `aura` CLI is the day-to-day surface of the toolchain ([RFC-012](/rfc/012)). From this repository, invoke it via Cargo:

```bash
cargo run -p aura-cli -- <command> [args]
```

## Commands (MVP)

| Command              | Purpose                                   |
| -------------------- | ----------------------------------------- |
| `check <file\|dir>`  | Parse + typecheck                         |
| `build <file\|dir>`  | Emit native binary (`-o` for output path) |
| `run <file\|dir>`    | Build and execute                         |
| `test <file\|dir>`   | Run `@test` functions                     |
| `emit-c <file\|dir>` | Emit C (advanced / debugging)             |

Examples:

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/test/smoke.aura
cargo run -p aura-cli -- build corpus/hello/main.aura -o target/aura/hello
```

## Inputs

- A **single `.aura` file**, or
- A **package directory** containing `aura.toml` and `src/`

Package mode unlocks multi-file compilation, imports, and path dependencies. See [Packages](./packages.md).

## Runtime and linking

`build` / `run` use the **C backend**: Aura → C → system `cc`, linked with `runtime/aura_rt.c`. LLVM IR remains the longer-term backend ([RFC-004](/rfc/004)).

## Diagnostics

Type and name errors print human-readable messages. Prefer `check` in editors/CI when you only need validation.

## Planned verbs

RFC-012 also describes `fmt`, `new`, package registry flows, and `doc`. Those are **not** all implemented yet — treat this page as “what works in-tree today.”

## Next

- [Packages](./packages.md)
- [Testing](./testing.md)
