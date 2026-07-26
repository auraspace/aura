---
title: Roadmap
section: Project
order: 70
summary: Execution status and a clear map of RFC Accepted vs implemented.
---

# Roadmap

Aura is **spec-first**: RFCs lock design; the compiler and runtime land vertical slices. This page is a **user-facing map**. The living engineering plan remains [`docs/roadmap.md`](https://github.com/auraspace/aura/blob/main/docs/roadmap.md) in the repo.

## How to read status

| Label                  | Meaning                                                 |
| ---------------------- | ------------------------------------------------------- |
| **RFC Accepted**       | Design decision is locked for implementers              |
| **Shipped (MVP)**      | Usable via `aura` CLI + corpus in this monorepo         |
| **Partial**            | Important pieces landed; not feature-complete vs RFC    |
| **Deferred / limited** | Accepted on paper; little or no user-facing runtime yet |

**Accepted ≠ fully implemented.** Always verify with corpus + CLI.

## RFC Accepted vs implemented

| RFC             | Title                      | RFC status | In code (approx.)       | Notes                                                                                                                              |
| --------------- | -------------------------- | ---------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| [000](/rfc/000) | Vision & design principles | Accepted   | **Shipped (docs)**      | Product north star                                                                                                                 |
| [001](/rfc/001) | Language specification     | Accepted   | **Partial → broad MVP** | Through **C22 bounded async slices**: classes, packages, captures, snapshots, task syntax; general async ownership/macros not full |
| [002](/rfc/002) | Type system                | Accepted   | **Partial**             | Null flow, generics, bounds, Result, fun types; deeper rules ongoing                                                               |
| [003](/rfc/003) | Memory & concurrency       | Accepted   | **Partial**             | GC mark/sweep, task frames, channels, bounded await and frame scans; general scheduler/ownership limited                           |
| [004](/rfc/004) | Compiler architecture      | Accepted   | **Partial**             | Rust toolchain + **C backend** default; LLVM later                                                                                 |
| [005](/rfc/005) | Package manager            | Accepted   | **Partial**             | Path deps + verified HTTPS/nested locked registry consumption; live publish/auth/signing remain open                               |
| [006](/rfc/006) | Runtime                    | Accepted   | **Partial**             | `aura_rt.c`, GC, exceptions/causes, task frames/channels, file I/O, FFI pins; full async I/O remains open                          |
| [007](/rfc/007) | Standard library           | Accepted   | **Partial**             | `std.io` / `assert` / Map·Set·HashMap<K,V> + Hashable + HOF + deterministic collection snapshots                                   |
| [008](/rfc/008) | Build system               | Accepted   | **Partial**             | `aura.toml`, profiles/cache APIs, package build/run/test; cross-host evidence remains open                                         |
| [009](/rfc/009) | Reflection / metadata      | Accepted   | **Deferred / limited**  | Not a day-one teach path                                                                                                           |
| [010](/rfc/010) | Plugins / macros           | Accepted   | **Deferred / limited**  | Not required for hello                                                                                                             |
| [011](/rfc/011) | Testing framework          | Accepted   | **Partial**             | `aura test` + `@test` MVP                                                                                                          |
| [012](/rfc/012) | CLI                        | Accepted   | **Partial**             | `new` / `init` / `version` / `check` / `build` / `run` / `test` / `emit-c`; `--` args pass-through (C12c)                          |
| [013](/rfc/013) | Binary distribution        | Accepted   | **Partial**             | `v0.1.0-alpha` tarballs + installer; v0.1.1-alpha signing and production acceptance remain open                                    |
| [014](/rfc/014) | Language server            | Draft      | **Deferred / planned**  | LSP contract drafted; implementation follows shared compiler queries                                                               |

## Compiler milestone band

Public README and repo `docs/roadmap.md` track **C0 → C22** with C22's implementation explicitly partial (bounded async/task slices and residual ownership debt).

| Band   | User-visible outcome                                                                                                                         |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| C0–C1  | Parse, typecheck, native hello via C backend                                                                                                 |
| C1b–C2 | Classes, interfaces, generics, null flow                                                                                                     |
| C3     | Structs, enums, tests, packages, arrays, imports, GC MVP                                                                                     |
| C4–C5  | GC refinements, std.io/assert, more Array/String APIs, diagnostics polish                                                                    |
| C6–C7  | Deep GC mark/sweep, Iterable, Map/Set, `Int?`/`Bool?`, Array field ownership                                                                 |
| C8–C9  | Generic iface/class mono, nested Array, HashMap(+resize), String+/interp, `is`                                                               |
| C10    | First-class funs/lambdas (expr/block), fun types, val captures MVP, Int HOF                                                                  |
| C11a–e | file I/O, Fun env free, `aura new`, substring, notes dogfood, **install/embed runtime**, 0.1 freeze                                          |
| C12a–t | **Done:** argv/stdin/exit, String tools, class·Array·var captures, HashMapStr, HOF str, tryReadFile, `examples/wc`, guide, install DX        |
| C13a–t | **Done:** method-on-temp, `Int.toString`, String array free, Fun/`var` String capture, registry K1 offline, eprint/tryWrite, signing note    |
| C20c–i | **Done/deferred:** mutable class/Array/Fun captures, snapshot iterator contract and fixtures; `Array<Interface>` and entry mutation deferred |

**Shipped:** tag `v0.1.0-alpha` + multi-OS tarballs ([release notes](https://github.com/auraspace/aura/blob/main/docs/releases/0.1.0-alpha.md)); C12 + **C13** batches closed.

**Next:** close the v0.1.1-alpha completion backlog: general async lowering/captures, async I/O and HTTP handles, live registry publication, FFI boundary gaps, and release evidence. See the [contract matrix](https://github.com/auraspace/aura/blob/main/docs/plans/v0.1.1-alpha/contract-matrix.tsv) and residual [debts](https://github.com/auraspace/aura/blob/main/agents/debts.md).

Exact bullet lists live in the root [README](https://github.com/auraspace/aura) and repo [`docs/roadmap.md`](https://github.com/auraspace/aura/blob/main/docs/roadmap.md).

## Near-term product shape

1. Keep **check / build / run / test** solid on packages
2. Grow **stdlib** and package ergonomics (generic collections, richer String)
3. Deepen **closures / GC** while C backend stays useful
4. Move toward **LLVM** without abandoning shippable binaries
5. Keep **user docs** aligned when features become teachable

## Related links

- [RFC catalog](/rfc) · [dependency graph](/rfc/graph)
- [Contributing](./contributing.md)
- [FAQ](./faq.md)
