# Aura RFC Index

Catalog of Request for Comments (RFCs) for the **Aura core**: language, compiler, runtime, standard library, and toolchain. Each RFC is an independent specification that can be reviewed, accepted, and evolved on its own track. Full prose lives in the linked files; this index is the **brief map** only.

## Product direction (locked)

| Axis                         | Decision                                                                                                                       |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| **Language model**           | Statically typed, **compiled** language                                                                                        |
| **Deploy model**             | Ship a **single executable file** per application (statically linked or equivalent one-artifact deploy)                        |
| **Scope**                    | **Core only** — language, type system, memory/concurrency, compiler, runtime, stdlib, packages, build, test, CLI, distribution |
| **Out of scope (for now)**   | Application frameworks, DI containers, ORM/data layers, HTTP app frameworks — may return later as separate RFCs                |
| **Toolchain implementation** | **Rust** — compiler, package manager, CLI, build/dist tooling                                                                  |
| **User-facing language**     | Aura (source programs and stdlib)                                                                                              |

Details and residual trade-offs are expanded in individual RFCs. Locked cross-cutting decisions below were frozen in the 2026-07-15 solid drafts. On **2026-07-16** all core RFCs (**000–013**) were reviewed and set to **Accepted**, with remaining open questions resolved or explicitly Deferred (see per-RFC §7).

### Locked design decisions (Solid Draft)

| Decision         | Choice                                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------------------------ |
| Object model     | Java-like classes; **final by default**; `companion`; value `struct`                                   |
| Nullability      | `T` non-null; `T?` nullable; flow-sensitive narrowing (locals)                                         |
| Errors           | Unchecked exceptions + `Result` (no checked throws)                                                    |
| Arrays / lambdas | `Array<T>`; `(params) => …`                                                                            |
| Strings          | Immutable                                                                                              |
| Memory           | Tracing GC; static-linked runtime default                                                              |
| Concurrency      | M:N tasks, channels, `async`/`await`; structured concurrency encouraged; race detector                 |
| Generics         | Monomorphization (+ vtable interface dispatch)                                                         |
| Impl coherence   | Interface impl in package of class **or** interface                                                    |
| Backend          | LLVM native; multipass compiler → query engine later                                                   |
| Toolchain        | Rust                                                                                                   |
| Deploy           | Single executable; minisign release signatures                                                         |
| Interop v1       | C ABI FFI                                                                                              |
| Targets v1       | Server + CLI; linux/mac/win × amd64/arm64                                                              |
| Macros v1        | Derives first; hygienic pattern macros later; proc in **process** sandbox; macros in normal packages   |
| Packages         | `aura.toml` + lockfile (**always committed**); flat CLI (`aura add`); flat names; reserve `std`/`aura` |
| Build            | No build scripts in MVP; LTO opt-in; no stable intermediate lib format yet                             |
| Stdlib           | Core only; **no** `std.http` in v1; small prelude; growable `List<T>` (+ builtin `Array`)              |
| Test             | `@test` package-private OK; same-process default; fixtures deferred                                    |
| Attributes       | Unknown `@attr` → hard **error**                                                                       |
| GC path          | free-all MVP → STW mark-sweep → concurrent later                                                       |
| Dist             | minisign; musl & win-arm64 **tier2**; toolchain via GitHub Releases                                    |

## RFC matrix

| RFC     | Title                      | Estimate     | Layer      | Status   | File                                           |
| ------- | -------------------------- | ------------ | ---------- | -------- | ---------------------------------------------- |
| RFC-000 | Vision & Design Principles | 15–20 pages  | Foundation | Accepted | [RFC-000](RFC-000-vision-design-principles.md) |
| RFC-001 | Language Specification     | 80–120 pages | Language   | Accepted | [RFC-001](RFC-001-language-specification.md)   |
| RFC-002 | Type System                | 40–60 pages  | Language   | Accepted | [RFC-002](RFC-002-type-system.md)              |
| RFC-003 | Memory Model & Concurrency | 40–80 pages  | Language   | Accepted | [RFC-003](RFC-003-memory-model-concurrency.md) |
| RFC-004 | Compiler Architecture      | 60–100 pages | Toolchain  | Accepted | [RFC-004](RFC-004-compiler-architecture.md)    |
| RFC-005 | Package Manager            | 20–40 pages  | Toolchain  | Accepted | [RFC-005](RFC-005-package-manager.md)          |
| RFC-006 | Runtime                    | 40–60 pages  | Runtime    | Accepted | [RFC-006](RFC-006-runtime.md)                  |
| RFC-007 | Standard Library           | 40–80 pages  | Runtime    | Accepted | [RFC-007](RFC-007-standard-library.md)         |
| RFC-008 | Build System               | 20–40 pages  | Toolchain  | Accepted | [RFC-008](RFC-008-build-system.md)             |
| RFC-009 | Reflection & Metadata      | 30–50 pages  | Language   | Accepted | [RFC-009](RFC-009-reflection-metadata.md)      |
| RFC-010 | Plugin & Macro System      | 50–80 pages  | Language   | Accepted | [RFC-010](RFC-010-plugin-macro-system.md)      |
| RFC-011 | Testing Framework          | 20–40 pages  | Toolchain  | Accepted | [RFC-011](RFC-011-testing-framework.md)        |
| RFC-012 | CLI                        | 20–30 pages  | Toolchain  | Accepted | [RFC-012](RFC-012-cli.md)                      |
| RFC-013 | Binary Distribution        | 20–30 pages  | Toolchain  | Accepted | [RFC-013](RFC-013-binary-distribution.md)      |
| RFC-014 | Language Server            | 30–50 pages  | Toolchain  | Draft    | [RFC-014](RFC-014-language-server.md)          |

**Total estimate (core):** ~505–860 pages.

### Implementation pulse (2026-07-22)

Living execution status is [docs/roadmap.md](../roadmap.md) (compiler **C0–C19d**, plus **S2** release/toolchain work). RFCs stay design docs; each has a short **Toolchain today** note where relevant.

| Layer            | Shipped (subset)                                                                                   | Still deferred                   |
| ---------------- | -------------------------------------------------------------------------------------------------- | -------------------------------- |
| Language / types | classes, iface, generics, struct/enum, null ops, Array, packages, lambdas MVP, generic collections | async, richer captures, macros   |
| Compiler         | C backend + `aura check/build/run/test`, diagnostics, C19 generic substitution                     | LLVM, incremental                |
| Runtime          | embedded runtime, println/I/O, exceptions, Array ownership, GC mark/sweep                          | tasks, channels, concurrent GC   |
| Packages / CLI   | path + locked registry deps, core subcommands, release tooling                                     | publish, fmt, workspaces         |
| Stdlib / test    | `std.io` / `assert` / generic collections + HOFs, `@test`                                          | net, JSON, async tests, coverage |

## Synopsis (one glance per RFC)

| RFC         | Synopsis                                                                                                                        |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------- |
| **RFC-000** | Vision, principles, non-goals: compiled Aura, single-file ship, core-only scope, toolchain in Rust.                             |
| **RFC-001** | Surface syntax and core language semantics: lexical rules, grammar, declarations, expressions, modules, visibility.             |
| **RFC-002** | Static type system: kinds, nullability/errors, generics, traits/interfaces, inference, assignability, soundness goals.          |
| **RFC-003** | Memory strategy, sharing rules, async/tasks, concurrency primitives, and data-race policy for single-binary programs.           |
| **RFC-004** | Compiler pipeline implemented in **Rust**: parse → typecheck → IR → native codegen, incremental build, diagnostics.             |
| **RFC-005** | Packages: manifest, lockfile, resolver, **GitHub-backed registry**, workspaces, publish — reproducible deps.                    |
| **RFC-006** | Runtime support linked into the final binary: scheduler, I/O, alloc (and GC if chosen), panic, FFI as required.                 |
| **RFC-007** | Standard library for servers and CLIs: collections, I/O, net, JSON, log, sync, crypto baseline — no app framework.              |
| **RFC-008** | Build graph and profiles: targets, features, caching, cross-compile — produces one deployable artifact by default.              |
| **RFC-009** | Attributes/annotations and metadata retention for language tooling, derives, and optional runtime type info.                    |
| **RFC-010** | Hygienic/declarative macros, procedural derives, and sandboxed compiler plugins (Rust-hosted where appropriate).                |
| **RFC-011** | Built-in testing: discovery, assertions, async tests, integration layout, coverage hooks.                                       |
| **RFC-012** | Unified `aura` CLI (Rust): new/build/run/test/check/fmt/pkg — one entrypoint for daily workflow.                                |
| **RFC-013** | How toolchain and apps are released: platform matrix, installers, signing, self-update, single-file app packaging.              |
| **RFC-014** | Editor integration via LSP: shared compiler analysis, workspace snapshots, diagnostics, navigation, completion, and safe edits. |

## Implementation stack (core)

```text
┌─────────────────────────────────────────────────────────┐
│  Aura source  →  aura CLI / build (Rust)                │
│       → compiler frontend+backend (Rust)                │
│       → link runtime + user code                        │
│       → single executable artifact                      │
└─────────────────────────────────────────────────────────┘
         │
         ├── stdlib (Aura)
         ├── packages (Aura ecosystem)
         └── tests via same toolchain
```

- **Rust** implements the compiler, build orchestration, package manager, CLI, and distribution tooling.
- **Aura** is what application and library authors write.
- Default release path: **one file** you copy and run.

## Proposed dependency graph

```text
RFC-000  Vision
   │
   ├─► RFC-001  Language Spec
   │      ├─► RFC-002  Type System
   │      ├─► RFC-003  Memory & Concurrency
   │      ├─► RFC-009  Reflection & Metadata
   │      └─► RFC-010  Plugin & Macro
   │
   ├─► RFC-004  Compiler (Rust)  ◄── RFC-001,002,003,009,010
   │      └─► RFC-008  Build System
   │
   ├─► RFC-006  Runtime   ◄── RFC-001,003
   │      └─► RFC-007  Standard Library
   │
   ├─► RFC-005  Package Manager
   ├─► RFC-011  Testing
   ├─► RFC-012  CLI (Rust)  ◄── RFC-005,008,011,013
   ├─► RFC-013  Binary Distribution
   └─► RFC-014  Language Server  ◄── RFC-004,008,012
```

## Recommended writing order (waves)

| Wave | RFCs                        | Why                                                            |
| ---- | --------------------------- | -------------------------------------------------------------- |
| 0    | RFC-000                     | Lock vision, single-file ship, Rust toolchain, core-only scope |
| 1    | RFC-001, 002, 003           | Language core                                                  |
| 2    | RFC-009, 010, 004, 006      | Metadata + macros + Rust compiler + runtime                    |
| 3    | RFC-005, 007, 008, 012, 013 | Packages, stdlib, build, CLI, distribution                     |
| 4    | RFC-011, RFC-014            | Testing and editor integration integrated with toolchain       |

## Status legend

| Status       | Meaning                                |
| ------------ | -------------------------------------- |
| `Draft`      | Being written / skeleton               |
| `In Review`  | Open for internal review               |
| `Accepted`   | Direction locked; implement against it |
| `Frozen`     | Stable; errata only                    |
| `Rejected`   | Not pursuing this direction            |
| `Superseded` | Replaced by a newer RFC                |

## Adding a new RFC

1. Copy [`TEMPLATE.md`](TEMPLATE.md) → `RFC-NNN-short-title.md` (next free number after the highest existing RFC).
2. Add a row to the matrix and a one-line synopsis above.
3. Update the dependency graph if needed.
4. Open review once required sections (Status, Motivation, Goals, Non-goals, Design) have real content.
