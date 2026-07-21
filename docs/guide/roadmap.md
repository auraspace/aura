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

| RFC             | Title                      | RFC status | In code (approx.)       | Notes                                                                                                         |
| --------------- | -------------------------- | ---------- | ----------------------- | ------------------------------------------------------------------------------------------------------------- |
| [000](/rfc/000) | Vision & design principles | Accepted   | **Shipped (docs)**      | Product north star                                                                                            |
| [001](/rfc/001) | Language specification     | Accepted   | **Partial → broad MVP** | Through **C13t**: classes, packages, lambdas + Fun/`var` String captures, String tools; async/macros not full |
| [002](/rfc/002) | Type system                | Accepted   | **Partial**             | Null flow, generics, bounds, Result, fun types; deeper rules ongoing                                          |
| [003](/rfc/003) | Memory & concurrency       | Accepted   | **Partial**             | GC mark/sweep + class heap; tasks/channels limited                                                            |
| [004](/rfc/004) | Compiler architecture      | Accepted   | **Partial**             | Rust toolchain + **C backend** default; LLVM later                                                            |
| [005](/rfc/005) | Package manager            | Accepted   | **Partial**             | Path deps + **C13** registry K1 offline (index/semver/fetch/lock); no live HTTPS / publish                    |
| [006](/rfc/006) | Runtime                    | Accepted   | **Partial**             | `aura_rt.c`, GC, exceptions, String Array free, file I/O, Fun env RC, argv/stdin/exit                         |
| [007](/rfc/007) | Standard library           | Accepted   | **Partial**             | `std.io` (+ eprint/tryWriteFile) / `assert` / Map·Set·HashMap<K,V> + Hashable + HOF + `Int.toString` / case   |
| [008](/rfc/008) | Build system               | Accepted   | **Partial**             | `aura.toml` package build/run/test                                                                            |
| [009](/rfc/009) | Reflection / metadata      | Accepted   | **Deferred / limited**  | Not a day-one teach path                                                                                      |
| [010](/rfc/010) | Plugins / macros           | Accepted   | **Deferred / limited**  | Not required for hello                                                                                        |
| [011](/rfc/011) | Testing framework          | Accepted   | **Partial**             | `aura test` + `@test` MVP                                                                                     |
| [012](/rfc/012) | CLI                        | Accepted   | **Partial**             | `new` / `init` / `version` / `check` / `build` / `run` / `test` / `emit-c`; `--` args pass-through (C12c)     |
| [013](/rfc/013) | Binary distribution        | Accepted   | **Partial**             | `v0.1.0-alpha` tarballs + `install.sh` / `avm`; no Windows matrix, signing, or self-update                    |

## Compiler milestone band

Public README and repo `docs/roadmap.md` track **C0 → C13t** batch closed (dogfood, Fun/`var` String captures, registry K1 offline).

| Band   | User-visible outcome                                                                                                                      |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| C0–C1  | Parse, typecheck, native hello via C backend                                                                                              |
| C1b–C2 | Classes, interfaces, generics, null flow                                                                                                  |
| C3     | Structs, enums, tests, packages, arrays, imports, GC MVP                                                                                  |
| C4–C5  | GC refinements, std.io/assert, more Array/String APIs, diagnostics polish                                                                 |
| C6–C7  | Deep GC mark/sweep, Iterable, Map/Set, `Int?`/`Bool?`, Array field ownership                                                              |
| C8–C9  | Generic iface/class mono, nested Array, HashMap(+resize), String+/interp, `is`                                                            |
| C10    | First-class funs/lambdas (expr/block), fun types, val captures MVP, Int HOF                                                               |
| C11a–e | file I/O, Fun env free, `aura new`, substring, notes dogfood, **install/embed runtime**, 0.1 freeze                                       |
| C12a–t | **Done:** argv/stdin/exit, String tools, class·Array·var captures, HashMapStr, HOF str, tryReadFile, `examples/wc`, guide, install DX     |
| C13a–t | **Done:** method-on-temp, `Int.toString`, String array free, Fun/`var` String capture, registry K1 offline, eprint/tryWrite, signing note |

**Shipped:** tag `v0.1.0-alpha` + multi-OS tarballs ([release notes](https://github.com/auraspace/aura/blob/main/docs/releases/0.1.0-alpha.md)); C12 + **C13** batches closed.

**Next:** live registry HTTPS + nested deps; generic collection HOFs; residual [debts](https://github.com/auraspace/aura/blob/main/agents/debts.md).

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
