# RFC-004: Compiler Architecture

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| **RFC**      | 004                                         |
| **Title**    | Compiler Architecture                       |
| **Status**   | Accepted                                    |
| **Layer**    | Toolchain                                   |
| **Authors**  |                                             |
| **Created**  | 2026-07-15                                  |
| **Updated**  | 2026-07-16                                  |
| **Estimate** | 60–100 pages                                |
| **Depends**  | RFC-001, RFC-002, RFC-003, RFC-009, RFC-010 |
| **Blocks**   | RFC-008, RFC-012, RFC-013                   |

---

## 1. Abstract

This RFC describes the **Aura compiler**, implemented in **Rust**: pipeline from source through parse, macro expansion, name resolution, type checking, HIR/MIR, optimization, and **LLVM** codegen, then object emission for linking with the Aura runtime into a **single native binary**. It covers incremental compilation strategy, diagnostics, IR layering, and bootstrapping.

## 2. Motivation

### 2.1 Problem statement

Aura needs a production-grade compiler path that supports Java-like OOP, nullability, generics monomorphization, async lowering, GC safepoints, and excellent errors—without waiting for self-host.

### 2.2 Why now

Compiler shape constrains build system, CLI, macros, and runtime ABI.

### 2.3 Success metrics

| Metric      | Target                                                 |
| ----------- | ------------------------------------------------------ |
| Correctness | Spec examples compile and run                          |
| Diagnostics | Spanned, multi-note, fix suggestions for common errors |
| Incremental | Edit-local rebuilds faster than full rebuild           |
| Determinism | Same inputs → same outputs (modulo path maps)          |

## 3. Goals

- Correct, fast, incremental compilation in Rust.
- Excellent diagnostics (error codes + suggestions).
- **LLVM** as the v1 native backend.
- Hooks for macros/attributes (RFC-010) and metadata (RFC-009).
- Clear IR boundaries for testing and `--emit`.

## 4. Non-goals

- Full self-host day-one.
- Detailed IDE LSP architecture (may share crates; separate doc).
- Cranelift/VM as primary v1 backend (optional later).
- Bit-identical reproducibility across host OS without containerized toolchains (best-effort).

## 5. Prior art & alternatives

| Compiler              | Notes               | Take                     |
| --------------------- | ------------------- | ------------------------ |
| rustc / rust-analyzer | Query engines, LLVM | Architecture inspiration |
| Kotlin/JVM & K/N      | Classes + native    | Lowering patterns        |
| Go compiler           | Fast whole-program  | Simplicity contrast      |
| Cranelift             | Fast codegen        | Future dev backend       |
| GCC/LLVM              | Industry backends   | **LLVM chosen**          |

## 6. Design

### 6.1 Overview — pipeline

```text
Source (.aura)
  → Lex / Parse                    (AST)
  → Attribute collect
  → Macro expansion (RFC-010)      (AST')
  → Name resolution                (AST' + resolutions)
  → Type check (RFC-002)           (typed HIR)
  → HIR lowering
  → MIR (CFG, async state machines, mono instantiation plan)
  → Optimizations (MIR-level)
  → LLVM IR codegen (+ GC safepoints, personality for exceptions)
  → Object files
  → Link with runtime (RFC-006) + user objs  →  single executable (RFC-008/013)
```

Implementation language: **Rust** crates, e.g. `aura-lexer`, `aura-parser`, `aura-hir`, `aura-mir`, `aura-llvm`, `aura-driver`.

### 6.2 Frontend

| Topic          | Decision                                                                |
| -------------- | ----------------------------------------------------------------------- |
| Parser         | Hand-written recursive descent + Pratt expressions                      |
| Error recovery | Synchronize on `;`, `}`, keywords; emit partial AST                     |
| AST stability  | Unstable across versions; tools use HIR or stable syntax tree API later |
| Encoding       | UTF-8; spans as byte offsets + file id                                  |

### 6.3 Name resolution & modules

- Resolve packages via manifest graph (RFC-005/008).
- Per-package symbol tables; visibility checks (pub/package/private/protected).
- Multi-file packages merge into one resolution universe.
- Cycle detection across packages → hard error.

### 6.4 Type checker architecture

- **Multi-pass with caching first**, evolving toward a **query-based** engine (Salsa-like) for incremental and IDE reuse (not full query engine on day one).
- Unit of invalidation: **item** (function/class member) preferred over whole file when possible.
- Nullability, inference, overload resolution per RFC-002.
- Outputs: typed HIR + method resolution tables + mono obligations.

### 6.5 Intermediate representations

| IR           | Purpose                                                   | Form         |
| ------------ | --------------------------------------------------------- | ------------ |
| Token stream | Macros, lex                                               | Linear       |
| AST          | Syntax, untyped                                           | Tree         |
| HIR          | Typed high-level, classes resolved                        | Tree + types |
| MIR          | CFG, lowers async, virtual calls explicit, mono instances | Graph        |
| LLVM IR      | Backend                                                   | SSA          |

**Monomorphization:** MIR/LLVM generation produces specialized copies for concrete generic instantiations; shared code for interface dispatch via vtables/itables.

### 6.6 Macro & plugin integration points

1. Pre-expansion attribute discovery.
2. Declarative macro expand during AST→AST' (RFC-010).
3. Derive macros produce synthetic items before typecheck when possible; type-aware derives may run after partial resolution (phased).
4. Proc plugins (later): out-of-process sandbox, stable AST API subset.

### 6.7 Diagnostics framework

- Every error: primary span, message, optional notes/labels, **error code** (`E0001`…).
- Suggestions: insert `?`, add import, make class `open`, etc.
- ICE: polite message + bug report template; no silent swallow.
- Render: terminal colors via CLI; JSON for tooling (`aura check --format=json`).

### 6.8 Backend strategy

| Backend        | v1                |
| -------------- | ----------------- |
| **LLVM**       | **Primary — yes** |
| Cranelift      | Future optional   |
| Bytecode VM    | Not v1            |
| Transpile to C | Not planned       |

**LLVM responsibilities:**

- Native codegen for target triples (linux/mac/win × amd64/arm64).
- Exception handling personality compatible with Aura exceptions.
- GC integration: safepoint polls, stack maps / statepoints (exact scheme TBD with RFC-006).
- Debug info: DWARF; Windows PDB path TBD.

### 6.9 Incremental & parallel compilation

- Dep graph: package → file → item queries.
- Parallel typecheck of independent items where safe.
- On-disk cache: `target/` hashed by source + compiler version + flags (RFC-008).
- Parallel LLVM codegen per codegen unit (CGU).

### 6.10 Metadata & debug info

- Emit DWARF for source-level debug.
- Optional reflection metadata sections (RFC-009) gated by retention.
- Strip options for release (`aura build --release`).

### 6.11 Bootstrapping

| Stage | Compiler            | Hosted on                      |
| ----- | ------------------- | ------------------------------ |
| Now   | Rust implementation | Stable Rust toolchain          |
| Later | Partial self-host   | Optional; not gate for Aura v1 |

### 6.12 Examples

```text
aura check
aura build --emit=mir
aura build --emit=llvm-ir -o /tmp/out.ll
aura build --release -o hello
```

### 6.13 Error model / edge cases

| Topic           | Policy                                                     |
| --------------- | ---------------------------------------------------------- |
| ICE             | Non-zero exit; log path                                    |
| Determinism     | Sorted iteration in unstable maps; explicit RNG seeds none |
| Resource limits | Recursion depth, mono instantiation caps                   |
| Target unknown  | Clear error listing supported triples                      |

### 6.14 Compatibility & migration

- IR formats unstable unless versioned for cache.
- Stable: CLI flags subset (RFC-012), error code meanings (additive).
- Breaking lowering changes allowed pre-1.0 with notes.

## 7. Open questions

| #   | Question                                        | Options                            | Owner       | Status                                                                       |
| --- | ----------------------------------------------- | ---------------------------------- | ----------- | ---------------------------------------------------------------------------- |
| 1   | GC statepoint style vs shadow stack             |                                    | Compiler+RT | **Resolved** — shadow stack on C backend; LLVM statepoints with LLVM backend |
| 2   | Full Salsa from day one vs multipass→query      | multipass first, evolve to queries | Compiler    | **Resolved**                                                                 |
| 3   | CGU partitioning heuristic                      |                                    | Compiler    | **Resolved** — package (or file group) as CGU default; refine later          |
| 4   | Exception model on LLVM (Itanium ABI vs custom) |                                    | Compiler    | **Resolved** — setjmp/custom on C backend; Itanium personality on LLVM       |

## 8. Rationale & trade-offs

Rust toolchain maximizes delivery speed and memory safety of the compiler itself. LLVM maximizes portability and optimization quality for single-binary ship. Layered IRs enable testing async lowering and mono without waiting for full codegen. Cost: binary size of toolchain and compile latency—mitigated by incremental builds and future Cranelift for dev if needed.

## 9. Unresolved / future work

- LSP architecture doc
- Compiler self-profiling
- Cranelift backend experiment
- Distributed build cache

## 10. Security & safety considerations

- Compiler must not execute arbitrary build scripts without policy (RFC-008).
- Macro/plugin sandbox (RFC-010).
- Path traversal in diagnostics suppressed.
- Supply chain: reproducible builds best-effort; signed releases (RFC-013).

## 11. Implementation plan (optional)

Long-term backend remains **LLVM**. The living milestone table is [docs/roadmap.md](../roadmap.md).

| Phase | Scope                                                                  | Status (2026-07-20)                                                                                  |
| ----- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| C0    | Parse + typecheck subset → `aura check`                                | **Done**                                                                                             |
| C1    | Native hello + runtime link                                            | **Done** via interim **C backend** (`emit-c` + system `cc` + `runtime/aura_rt.c`); LLVM still target |
| C2    | Generics mono + classes/interfaces                                     | **Done** (C2a–C2e)                                                                                   |
| C3    | Packages, Array, exceptions, GC MVP (not full async)                   | **Done** as C3a–C3z slices; async/incremental deferred                                               |
| C4–C5 | Equality, std, Array/String APIs, GC refinements                       | **Done**                                                                                             |
| C6–C9 | Deep GC, Iterable, collections, generic iface/class mono, String+/is   | **Done**                                                                                             |
| C10   | First-class funs/lambdas, fun types, val captures MVP, Int HOF         | **Done** (C10a–j)                                                                                    |
| Later | Richer captures, Array-of-iface, registry client, LLVM, channels/tasks | Open (see `agents/debts.md`)                                                                         |

## 12. References

- LLVM LangRef; GC statepoints docs
- rustc driver architecture (conceptual)
- RFC-001, 002, 003, 006, 008, 009, 010

---

## Changelog

| Date       | Author | Change                                                                                       |
| ---------- | ------ | -------------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock shadow-stack→statepoints, CGU=package, exception ABI path                               |
| 2026-07-16 |        | Status → **Accepted** — Review: Rust multipass + C-backend interim matches shipped toolchain |
| 2026-07-16 |        | §11: C backend interim + C0–C4t status                                                       |
| 2026-07-15 |        | Initial skeleton                                                                             |
| 2026-07-15 |        | Solid draft: Rust pipeline, LLVM primary                                                     |
| 2026-07-15 |        | Lock multipass→query evolution                                                               |
