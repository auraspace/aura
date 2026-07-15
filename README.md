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
cargo run -p aura-cli -- test corpus/test/smoke.aura    # run @test functions
cargo run -p aura-cli -- build corpus/hello/main.aura -o target/aura/hello
cargo run -p aura-cli -- check corpus/multi             # multi-file + aura.toml
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/multi              # package-wide @test
cargo run -p aura-cli -- run corpus/import/app          # import + path dep
```

C1 uses a **C backend** (`aura emit-c` + system `cc`) linked with `runtime/aura_rt.c`. LLVM IR is the longer-term path (RFC-004).

## Status

- **RFC-000** Accepted (vision locked)
- **RFC-001 §6.0** MVP surface for C0–C1
- **Compiler C0+** lexer + parser + name resolution + typecheck
- **Compiler C1** `aura build` / `aura run` → native hello binary (C backend)
- **Compiler C1b** `class` primary constructor, methods, `this`, field access
- **Compiler C2a** `interface` + implements + interface-typed calls (closed-world C dispatch)
- **Compiler C2b** generics: `class Box<T>`, `fun id<T>`, monomorphized C (`Box_String`, …)
- **Compiler C2c** type-arg inference: `Box("hi")`, `id(x)`, annotation-driven
- **Compiler C2d** nullability flow (`if (x != null)`) and force-unwrap `!!`
- **Compiler C2e** type-param bounds (`T : Named`) and `where T : A, T : B`
- **Compiler C3a** `struct` value types (primary ctor fields + methods; no implements)
- **Compiler C3b** `enum` + `match` + generic `Result<T, E>` (exhaustive arms)
- **Compiler C3c** `throw` / `try` / `catch` / `finally` (payloads: String, Int, Bool)
- **Compiler C3d** `aura test` with `@test`, `assert`, `assert_eq` (Int/String/Bool)
- **Compiler C3e** multi-file same package + minimal `aura.toml` (`check`/`build`/`run`/`test` on dir)
- **Compiler C3f** `import` + `pub` visibility + `[dependencies]` path deps
- **Compiler C3g** throw/catch class & struct values (`aura_throw_obj`)
- **Compiler C3h** `for (i in start..end)` exclusive Int range loops
- **Compiler C3i** `break` / `continue` inside loops
- **Compiler C3j** builtin `Array<T>` (`Int`/`Bool`/`String`; `len` / `get` / `set`)
- **DX** Pretty diagnostics (`path:line:col` + source snippet)
- **Debts** Tracked in [`agents/debts.md`](agents/debts.md)
- **Next:** for-in over Array, GC, registry/lockfile, LLVM

## Links

- [Roadmap](docs/roadmap.md)
- [RFC index](docs/rfc/README.md)
- [Site README](site/README.md)
- [Crates README](crates/README.md)
