# Aura

**Aura** is a statically typed, compiled language (classes, null-safe types, lightweight tasks, and GC) that ships as a **single native executable**. The **toolchain is Rust + LLVM**; application code is Aura.

This repository currently holds:

| Path                                 | Purpose                                                              |
| ------------------------------------ | -------------------------------------------------------------------- |
| [`docs/guide/`](docs/guide/)         | User guide (site `/docs`)                                            |
| [`docs/rfc/`](docs/rfc/)             | Language & toolchain RFCs                                            |
| [`docs/roadmap.md`](docs/roadmap.md) | Execution phases (P0–P3, compiler C0–C6j)                            |
| [`site/`](site/)                     | Homepage + docs + RFC site (Vite + React)                            |
| [`crates/`](crates/)                 | Rust toolchain (`aura` CLI) — check / build / run / test (C backend) |
| [`corpus/`](corpus/)                 | Sample `.aura` programs for the compiler                             |
| [`std/`](std/)                       | Minimal std packages (`io`, `assert`)                                |
| [`runtime/`](runtime/)               | Linked C runtime (`aura_rt.c`)                                       |

**License:** [MIT](LICENSE)

## Quick start

### Docs site

Homepage, user docs, and RFC catalog: **https://aura.fadosoft.com** (Cloudflare Pages).

`site/` is a pnpm workspace package — install from the repo root, then use root scripts:

```bash
pnpm install
pnpm site:dev      # http://localhost:5173
pnpm site:test
pnpm site:build
```

### Compiler (through C6j)

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
cargo run -p aura-cli -- run corpus/std_io/app          # std.io.println (C3z)
cargo run -p aura-cli -- run corpus/std_io/prelude      # auto-prelude std.io (C4g)
cargo run -p aura-cli -- run corpus/std_assert/app      # std.assert (C4h)
```

Native builds use a **C backend** (`aura emit-c` + system `cc`) linked with `runtime/aura_rt.c`. LLVM IR is the longer-term path (RFC-004).

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
- **Compiler C3k** `for (x in array)` element iteration over `Array<T>`
- **Compiler C3l** inclusive range `for (i in a..=b)`
- **Compiler C3m** `Array.push` with capacity grow (`realloc`)
- **Compiler C3n** `import path as Alias` → `Alias.fun(...)` qualified calls
- **Compiler C3o** package-prefixed free-function C symbols (same name across packages)
- **Toolchain C3p** `aura.lock` for path dependencies (verify + write)
- **Compiler C3q** Bare comparison C emit (avoids clang `-Wparentheses-equality`)
- **Compiler C3r** `Array.pop` (returns last element; empty throws)
- **Runtime C3s** Free exception object payloads on catch clear
- **Compiler C3t** Free owned `Array` buffers at scope end / before return
- **Compiler C3u** `import … as Alias` → `Alias.Type` / `Alias.Type(...)`
- **Compiler C3v** Package-prefixed class/enum C symbols (same name across packages)
- **Compiler C3w** `for (b in string)` over UTF-8 bytes as Int
- **Runtime C3x** GC MVP: `aura_gc_alloc` + free-all on process exit
- **Compiler C3y** Class instances as GC heap references (`struct` remains by-value)
- **Stdlib C3z** Minimal `std.io.println` (`std/io`, path dep; builtins remain)
- **Compiler C4a** Class identity `==` / `!=` (reference / pointer; corpus)
- **Compiler C4b** Nullable class `Class?` (correct heap pointer C emit + flow)
- **Compiler C4c** `Array` of class heap references
- **Compiler C4d** Package-prefixed interface C symbols (same name across packages)
- **Compiler C4e** String content equality (`strcmp`; class stays identity)
- **Compiler C4f** `Array.clear` (len=0, keep capacity)
- **CLI C4g** Auto-prelude `std.io` for package builds (`AURA_STD` or walk-up)
- **Stdlib C4h** `std.assert` package + auto path resolve for `std.*` imports
- **Compiler C4i** Reject struct/enum/interface equality in sema
- **CLI C4j** Nested path deps recorded in `aura.lock` (`# transitive`)
- **Compiler C4k** Type-param mono as heap pointers; field-chain method receivers
- **Compiler C4l** `else if` chaining
- **Compiler C4m** Null coalesce `?:`
- **Compiler C4n** `Array.isEmpty`
- **Compiler C4o** `Array.reserve`
- **Compiler C4p** `String.len` (UTF-8 byte length)
- **Compiler C4q** `Array` of struct by-value elements
- **Compiler C4r** Free Array buffer on owner reassignment
- **Compiler C4s** Safe call `?.` on nullable receivers
- **Compiler C4t** If as expression (value from last branch expr)
- **Compiler C4u** Nested mono audit (skip open monomorphs, return-type resolve, C struct forwards)
- **Compiler C4v** `String.isEmpty()` (UTF-8 byte length == 0)
- **Compiler C4w** `String.charAt(i)` (UTF-8 byte as Int; OOB throws)
- **Compiler C4x** Clear diagnostic for unsupported Array element types (interface still)
- **Compiler C4y** Duck Iterable `for-in` (`len` field/method + `get(Int)`)
- **Runtime C4z** GC STW skeleton: root registry + `aura_gc_collect` mark (free-all still at shutdown)
- **Stdlib C5a** `std/collections` stub (Map/Set not yet; use Array)
- **Compiler C5b** Array ownership move on `val b = a` (source buffer zeroed)
- **Compiler C5c** Undefined-name diagnostics with `did you mean …?`
- **Docs C5d** C4u–C5d batch closed (plan/roadmap/debts)
- **Compiler C5e** Array move on assign `b = a`
- **Runtime C5f** GC collect sweep when roots registered
- **Codegen C5g** GC roots for heap-class locals
- **Compiler C5h–C5j** String.startsWith / contains / endsWith
- **Compiler C5k** Assign type mismatch expected/found
- **Runtime C5m** Builtin `gc_collect()` + roots corpus
- **Docs C5n** C5e–C5n batch closed
- **Runtime C6a** Deep GC mark (worklist scan of object bytes)
- **Compiler C6b** Array move into function/method params
- **Compiler C6c** Iterable protocol: `for-in` on iface `len`+`get`
- **Compiler C6d** Array return/call binding owns buffer
- **Runtime C6e** GC mark Array-of-class local/param buffers
- **Stdlib C6f** `std.collections` Map (String→Int)
- **Compiler C6g** `Array` of enum by-value (unit + generic `Result`)
- **Sema C6h** Multi-error collect in function bodies
- **Compiler C6i** Field Array ownership (ctor + var reassign move)
- **Docs C6j** C6a–C6j batch closed (plan/roadmap/debts)
- **Codegen C7a** `Int?`/`Bool?` tagged optional C emit; `Map.get` → `Int?`
- **DX** Pretty diagnostics (`path:line:col` + source snippet)
- **Debts** Tracked in [`agents/debts.md`](agents/debts.md)
- **Next:** Array field GC free/mark; generic Map/Set

## Links

- [Roadmap](docs/roadmap.md)
- [RFC index](docs/rfc/README.md)
- [Site README](site/README.md)
- [Crates README](crates/README.md)
