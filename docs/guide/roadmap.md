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

| RFC             | Title                      | RFC status | In code (approx.)                                 | Notes                                                                                                                                                                                                  |
| --------------- | -------------------------- | ---------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [000](/rfc/000) | Vision & design principles | Accepted   | **Shipped (docs)**                                | Product north star                                                                                                                                                                                     |
| [001](/rfc/001) | Language specification     | Accepted   | **Partial → broad MVP**                           | Through **C22 bounded async slices**: classes, packages, captures, snapshots, task syntax; general async ownership/macros not full                                                                     |
| [002](/rfc/002) | Type system                | Accepted   | **Partial**                                       | Null flow, generics, bounds, Result, fun types; deeper rules ongoing                                                                                                                                   |
| [003](/rfc/003) | Memory & concurrency       | Accepted   | **Partial**                                       | GC mark/sweep, task frames, channels, bounded await and frame scans; general scheduler/ownership limited                                                                                               |
| [004](/rfc/004) | Compiler architecture      | Accepted   | **IR boundary shipped; C + scalar LLVM backends** | Checked IR/MIR/state-machine contracts are backend-neutral; C remains the compatibility backend and LLVM covers complete scalar MIR                                                                    |
| [005](/rfc/005) | Package manager            | Accepted   | **Partial**                                       | Path deps + verified HTTPS/direct Git-origin consumption; workspaces and proxy serving remain out of scope                                                                                             |
| [006](/rfc/006) | Runtime                    | Accepted   | **Partial**                                       | `runtime.c`, GC, exceptions/causes, task frames/channels, file I/O, FFI pins; full async I/O remains open                                                                                              |
| [007](/rfc/007) | Standard library           | Accepted   | **Partial**                                       | `std.io` / `assert` / Map·Set·HashMap<K,V> + Hashable + HOF + deterministic collection snapshots                                                                                                       |
| [008](/rfc/008) | Build system               | Accepted   | **Partial**                                       | `aura.toml`, profiles/cache APIs, package build/run/test; cross-host evidence remains open                                                                                                             |
| [009](/rfc/009) | Reflection / metadata      | Accepted   | **Deferred / limited**                            | Not a day-one teach path                                                                                                                                                                               |
| [010](/rfc/010) | Plugins / macros           | Accepted   | **Deferred / limited**                            | Not required for hello                                                                                                                                                                                 |
| [011](/rfc/011) | Testing framework          | Accepted   | **Partial**                                       | `aura test` + `@test`, substring filters, race mode, and JSON reports; parallel/async/coverage remain open                                                                                             |
| [012](/rfc/012) | CLI                        | Accepted   | **Partial**                                       | `new` / `init` / `version` / `check` / `build` / `run` / `test` / `race` / `fmt` / `emit-c` / `update`; package UX remains bounded                                                                     |
| [013](/rfc/013) | Binary distribution        | Accepted   | **Partial**                                       | Prepared `v0.1.1-alpha.8` tarballs + installer; tier-2 targets and broader native evidence remain open                                                                                                 |
| [014](/rfc/014) | Language server            | Draft      | **MVP shipped**                                   | `auralsp`/`aura lsp` stdio server with sync, diagnostics, formatting, symbols, completion, hover, navigation, references, rename, and safe code actions; deeper package/binding precision remains open |

## Compiler milestone band

Public README and repo `docs/roadmap.md` track **C0 → C22**. The compiler
architecture milestone is separate: Checked IR/MIR lowering and semantic
coverage are backend-neutral even while the C runtime/backend remains alpha.

| Band   | User-visible outcome                                                                                                                                           |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C0–C1  | Parse, typecheck, native hello via C backend                                                                                                                   |
| C1b–C2 | Classes, interfaces, generics, null flow                                                                                                                       |
| C3     | Structs, enums, tests, packages, arrays, imports, GC MVP                                                                                                       |
| C4–C5  | GC refinements, std.io/assert, more Array/String APIs, diagnostics polish                                                                                      |
| C6–C7  | Deep GC mark/sweep, Iterable, Map/Set, `Int?`/`Bool?`, Array field ownership                                                                                   |
| C8–C9  | Generic iface/class mono, nested Array, HashMap(+resize), String+/interp, `is`                                                                                 |
| C10    | First-class funs/lambdas (expr/block), fun types, val captures MVP, Int HOF                                                                                    |
| C11a–e | file I/O, Fun env free, `aura new`, substring, notes dogfood, **install/embed runtime**, 0.1 freeze                                                            |
| C12a–t | **Done:** argv/stdin/exit, String tools, class·Array·var captures, String HashMap compatibility helper, HOF str, tryReadFile, `examples/wc`, guide, install DX |
| C13a–t | **Done:** method-on-temp, `Int.toString`, String array free, Fun/`var` String capture, registry K1 offline, eprint/tryWrite, signing note                      |
| C20c–j | **Done:** mutable class/Array/Fun captures, snapshot and invalidation-checked live iterators, `Array<Interface>`, and HashMap entry mutation                   |

**Shipped:** tags `v0.1.0-alpha` and `v0.1.1-alpha.8` + multi-OS tarballs; C12,
**C13**, S2, and the bounded v0.1.1-alpha.8 release scope are closed.

**Next:** post-release follow-up covers general async lowering/captures, richer
async I/O and HTTP handles, FFI boundary gaps, and tier-2/native evidence. The
package origin contract is complete; proxy serving and checksum-database/index
services are intentionally outside scope. See the [contract matrix](https://github.com/auraspace/aura/blob/main/docs/plans/v0.1.1-alpha/contract-matrix.tsv) and residual [debts](https://github.com/auraspace/aura/blob/main/agents/debts.md).

Exact bullet lists live in the root [README](https://github.com/auraspace/aura) and repo [`docs/roadmap.md`](https://github.com/auraspace/aura/blob/main/docs/roadmap.md).

## Near-term product shape

1. Keep **check / build / run / test / fmt / race** solid on packages
2. Grow **stdlib** and package ergonomics (generic collections, richer String)
3. Deepen **closures / GC** while C backend stays useful
4. Implement LLVM/Cranelift lowering against the shipped Checked IR/MIR boundary without abandoning shippable C-alpha binaries
5. Keep **user docs** aligned when features become teachable

## Related links

- [RFC catalog](/rfc) · [dependency graph](/rfc/graph)
- [Contributing](./contributing.md)
- [FAQ](./faq.md)
