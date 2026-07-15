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

### Compiler C0 (`aura check`)

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura
```

## Status

- **RFC-000** Accepted (vision locked)
- **RFC-001 §6.0** MVP surface for C0–C1
- **Compiler C0** lexer + parser + `aura check` (no codegen yet)
- **Next:** C1 hello binary via LLVM (see roadmap)

## Links

- [Roadmap](docs/roadmap.md)
- [RFC index](docs/rfc/README.md)
- [Site README](site/README.md)
- [Crates README](crates/README.md)
