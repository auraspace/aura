# Aura

**Aura** is a statically typed, compiled language (Java-like classes, null-safe types, Go-like tasks/GC) that ships as a **single native executable**. The **toolchain is Rust + LLVM**; application code is Aura.

This repository currently holds:

| Path | Purpose |
| ---- | ------- |
| [`docs/rfc/`](docs/rfc/) | Language & toolchain RFCs |
| [`docs/roadmap.md`](docs/roadmap.md) | Execution phases (P0–P3, C0–C1) |
| [`site/`](site/) | Static RFC docs site (Vite + React) |
| [`crates/`](crates/) | Rust toolchain (`aura` CLI) — **C0**: parse / check |
| [`corpus/`](corpus/) | Sample `.aura` programs for the compiler |

**License:** [MIT](LICENSE)

## Quick start

### Docs site

```bash
pnpm site:dev      # http://localhost:5173
pnpm site:test
pnpm site:build
```

### Compiler C0+ / C1

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura   # parse + typecheck
cargo run -p aura-cli -- run corpus/hello/main.aura     # build & execute
cargo run -p aura-cli -- build corpus/hello/main.aura -o target/aura/hello
```

C1 uses a **C backend** (`aura emit-c` + system `cc`) linked with `runtime/aura_rt.c`. LLVM IR is the longer-term path (RFC-004).

## Status

- **RFC-000** Accepted (vision locked)
- **RFC-001 §6.0** MVP surface for C0–C1
- **Compiler C0+** lexer + parser + name resolution + typecheck
- **Compiler C1** `aura build` / `aura run` → native hello binary (C backend)
- **Compiler C1b** `class` primary constructor, methods, `this`, field access
- **Compiler C2a** `interface` + implements + interface-typed calls (closed-world C dispatch)
- **DX** Pretty diagnostics (`path:line:col` + source snippet)
- **Next:** generics (C2b), richer nullability, LLVM backend

## Links

- [Roadmap](docs/roadmap.md)
- [RFC index](docs/rfc/README.md)
- [Site README](site/README.md)
- [Crates README](crates/README.md)
