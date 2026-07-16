---
title: Getting started
section: Start
order: 20
summary: Clone the repo, build the CLI, and run Hello, Aura.
---

# Getting started

This guide assumes you are working from the **Aura repository**. A standalone installer is not shipped yet; the toolchain builds with Cargo.

## Prerequisites

- **Rust** toolchain (`cargo`, `rustc`) — current stable is fine
- A C compiler on `PATH` (`cc`, `clang`, or `gcc`) for the native link step
- **pnpm** only if you want the docs site locally

## Build the CLI

From the repository root:

```bash
cargo build -p aura-cli
# or, without installing:
cargo run -p aura-cli -- --help
```

The binary is the `aura` CLI (package `aura-cli`).

## Hello, Aura

A minimal program lives at `corpus/hello/main.aura`:

```aura
// C0 corpus — hello
package main

fun main() {
  println("Hello, Aura")
}
```

### Typecheck

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
```

### Run (compile + execute)

```bash
cargo run -p aura-cli -- run corpus/hello/main.aura
```

### Build a native binary

```bash
cargo run -p aura-cli -- build corpus/hello/main.aura -o target/aura/hello
./target/aura/hello
```

Native builds emit C, compile with the system `cc`, and link `runtime/aura_rt.c`.

## More corpus samples

| Command | What it shows |
| ------- | ------------- |
| `run corpus/multi` | Multi-file package + `aura.toml` |
| `test corpus/test/smoke.aura` | `@test` functions |
| `run corpus/import/app` | Path dependencies |
| `run corpus/std_io/app` | `std.io.println` |

Full list of compiler milestones is in the root [README](https://github.com/auraspace/aura).

## Docs site (optional)

```bash
pnpm site:dev      # http://localhost:5173
pnpm site:build
```

## Next

- [Language tour](./language-tour.md) — map of language guides
- [Types & nullability](./types-and-nullability.md)
- [CLI](./cli.md) — verb reference
- [FAQ](./faq.md)
