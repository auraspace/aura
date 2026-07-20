# RFC-000: Vision & Design Principles

| Field        | Value                      |
| ------------ | -------------------------- |
| **RFC**      | 000                        |
| **Title**    | Vision & Design Principles |
| **Status**   | Accepted                   |
| **Layer**    | Foundation                 |
| **Authors**  | Aura contributors          |
| **Created**  | 2026-07-15                 |
| **Updated**  | 2026-07-16                 |
| **Estimate** | 15–20 pages                |
| **Depends**  | —                          |
| **Blocks**   | RFC-001 … RFC-013          |

---

## 1. Abstract

**Aura** is a **statically typed, compiled** language for services, CLIs, workers, and libraries that ship as a **single native executable**. The user-facing language is class-based (Java-like) with **null-safe types**, **exceptions plus `Result`**, and a **Go-like runtime**: tracing GC and M:N lightweight tasks. The **toolchain is implemented in Rust** and lowers through **LLVM** to platform binaries.

This RFC locks product vision, non-goals, design principles, and the cross-RFC decision set that all subsequent RFCs must respect. It does not specify grammar, type rules, or compiler internals—those live in child RFCs.

## 2. Motivation

### 2.1 Problem statement

Teams building modern backends and tools often face a false choice:

| Stack pattern                    | Pain                                                                         |
| -------------------------------- | ---------------------------------------------------------------------------- |
| Dynamic / JIT server runtimes    | Fast iteration, weaker deploy guarantees, dual-language tooling (app vs ops) |
| Managed heavyweight platforms    | High productivity, larger footprint and ops surface                          |
| Systems languages with ownership | Excellent safety and perf, higher ceremony for everyday service code         |
| Transpiled ecosystems            | Large libraries, but runtime gaps and “two languages” (source vs runtime)    |

Aura targets a middle path: **Java-like productivity and object model**, **Go-like concurrency and GC**, **native single-file deploy**, with a **coherent first-party toolchain** (compiler, packages, build, test, CLI).

### 2.2 Why now

- Demand for **copy-one-binary** deploy (containers, edge agents, CLI distribution) is higher than ever.
- LLVM and Rust make a high-quality greenfield toolchain realistic without waiting for self-host.
- Language design can bake in **nullability** and a clear **error model** instead of bolting them on decades later.

### 2.3 Success metrics

| Metric                                   | Direction (MVP → v1)                                                                          |
| ---------------------------------------- | --------------------------------------------------------------------------------------------- |
| Time-to-hello (install → running binary) | Minutes, not hours                                                                            |
| Deploy artifact                          | One executable per app by default                                                             |
| Null-related defects                     | Prevented at compile time for non-`?` types                                                   |
| Concurrent I/O services                  | Expressible with tasks + channels without manual thread pools                                 |
| Toolchain self-containment               | `aura` CLI covers new/build/run/test/check/fmt/pkg                                            |
| Footprint                                | Competitive with Go-class services for comparable workloads (benchmark targets in later RFCs) |

## 3. Goals

**Product**

- Safe-by-default nullability and explicit escape hatches.
- Predictable performance and memory suitable for long-running servers and short-lived CLIs.
- Batteries-included **core** stdlib; frameworks stay out of core.
- Excellent diagnostics and day-one CLI workflow.

**Platform**

- Compiled Aura → single native binary (statically linked or equivalent one-artifact ship).
- Toolchain in **Rust**; language and stdlib in **Aura**.
- Reproducible packages and builds (`aura.toml` + lockfile).
- Cross-platform matrix: Linux, macOS, Windows × amd64/arm64 (v1).

## 4. Non-goals

- Replacing every language in every domain (no GPU/CUDA model, no browser/DOM first-class in v1).
- Application frameworks, DI containers, ORM/data layers, HTTP app frameworks in **core** RFCs (may return as separate ecosystem RFCs).
- Day-one self-hosting of the compiler in Aura.
- Full formal machine-checked semantics in MVP (prose + grammar + tests first).
- Node/npm or JVM bytecode as primary interop/runtime (C ABI FFI only for v1 interop).
- WASM as a v1 ship target (may appear later).

## 5. Prior art & alternatives

| Approach          | Pros                              | Cons                                  | Decision                                 |
| ----------------- | --------------------------------- | ------------------------------------- | ---------------------------------------- |
| TypeScript + Node | DX, ecosystem                     | Dual surface, deploy model            | Inspire DX, not runtime                  |
| Go                | Simple concurrency, single binary | Weaker null/type expressiveness       | Adopt GC + tasks spirit                  |
| Java / Kotlin     | Classes, tooling maturity         | Heavy runtime/ship story historically | Adopt class model + nullability ideas    |
| Rust              | Safety, LLVM story                | Ownership ceremony for many services  | Toolchain only; not user ownership model |
| Swift / C#        | Modern multi-paradigm             | Different deploy/ecosystem goals      | Selective surface inspiration            |

## 6. Design

### 6.1 Product vision

**One-liner:** Aura is a Java-like, null-safe, GC language with Go-like tasks that compiles to a single native binary via a Rust+LLVM toolchain.

**Narrative:** An engineer writes Aura with familiar classes and interfaces, marks nullable references explicitly, handles expected failures with `Result` and unexpected ones with exceptions, spawns tasks for concurrent I/O, and runs `aura build` to produce one file they can copy onto a server or ship as a CLI. No separate runtime install for end users of the app artifact; the GC and scheduler are **linked into the binary**.

### 6.2 Target users & use cases

| Persona                    | Primary use case                        | Priority |
| -------------------------- | --------------------------------------- | -------- |
| Systems / backend engineer | HTTP workers, queues, internal services | P0       |
| Library author             | Reusable packages on the Aura registry  | P0       |
| Platform / infra           | CLI tools, agents, daemons              | P1       |
| Full-stack / product eng   | Services behind existing gateways       | P1       |

### 6.3 Design principles

For each: **statement**, **rationale**, **implications**, **counter-example**.

#### P1 — Safety by default, escape hatches explicit

- **Statement:** Safe defaults (non-null types, checked boundaries); `unsafe`, raw FFI, and suppressed checks are explicit and auditable.
- **Rationale:** Most bugs in service code are mundane (null, races, bad error handling), not exotic.
- **Implications:** `T` is non-null; `T?` is opt-in; race detector in dev; FFI in dedicated modules.
- **Counter-example:** Implicit null on all references (classic Java) is rejected.

#### P2 — One artifact from source to production

- **Statement:** The default build produces a single executable embedding user code + runtime.
- **Rationale:** Operational simplicity beats multi-file runtime installs for many deploy paths.
- **Implications:** Runtime is a library linked in; dynamic plugin loading is non-default.
- **Counter-example:** “Install JRE/Node on every host” as the primary story is out of scope.

#### P3 — Predictable performance & memory

- **Statement:** GC and scheduler behavior is documentable; hot paths can use value types/arenas where the language allows.
- **Rationale:** Services need latency budgets, not only throughput peaks.
- **Implications:** Prefer simple object model; monomorphized generics where it matters; no hidden interpreter in production path for v1.
- **Counter-example:** Unbounded reflection-heavy frameworks as stdlib defaults.

#### P4 — Batteries included, modular core

- **Statement:** Stdlib covers collections, I/O, net primitives, JSON, log, sync, crypto baseline.
- **Rationale:** Cold-start ecosystems fail without basics; frameworks are opinionated and version-churny.
- **Implications:** Core RFCs stop at libraries, not app frameworks.
- **Counter-example:** Shipping a full web MVC stack inside stdlib.

#### P5 — Tooling is part of the language

- **Statement:** Format, test, package, and build are first-class CLI verbs with stable contracts.
- **Rationale:** Fragmented toolchains destroy DX more than missing syntax sugar.
- **Implications:** RFC-011/012/005/008 are not afterthoughts.
- **Counter-example:** “Community will invent the formatter” as the plan.

#### P6 — Progressive disclosure of complexity

- **Statement:** Simple programs stay simple; advanced features (attributes, macros, unsafe, FFI) appear when needed.
- **Rationale:** Onboarding cost dominates adoption.
- **Implications:** Hello-world needs no build-file ceremony beyond defaults; advanced IR/flags stay opt-in.
- **Counter-example:** Requiring macros or DI to print a line.

### 6.4 Pillar map (language → ecosystem)

```text
Language (001–003, 009–010)
    → Compiler / Build / CLI (004, 008, 012)
    → Runtime / Stdlib (006–007)
    → Packages (005, 013)
    → Testing (011)
```

### 6.5 Locked cross-RFC decisions

| Decision               | Choice                                                                                                     | Owning RFC(s)           |
| ---------------------- | ---------------------------------------------------------------------------------------------------------- | ----------------------- |
| Language model         | Statically typed, compiled                                                                                 | 000, 001                |
| Object model           | Java-like classes, inheritance, virtual methods, interfaces                                                | 001, 002                |
| Value types            | Distinct `struct` (value) vs `class` (ref) in v1                                                           | 001, 002, 003           |
| Classes default        | **Final by default**; `open` required to subclass                                                          | 001                     |
| Static members         | `companion` object (not free-floating `static` keyword as primary)                                         | 001                     |
| Nullability            | `T` non-null; `T?` nullable; flow-sensitive narrowing                                                      | 001, 002                |
| Error model            | **Unchecked** exceptions + `Result` for expected failures (no checked throws)                              | 001, 002                |
| Arrays                 | `Array<T>` (not `T[]`)                                                                                     | 001                     |
| Lambdas                | `(params) => expr` / block body                                                                            | 001                     |
| Integers               | Checked overflow in `dev`; explicit wrapping ops; release policy may elide checks with documented wrap ops | 001, 002                |
| Memory                 | Tracing GC                                                                                                 | 003, 006                |
| Strings                | Immutable `String`                                                                                         | 001, 003, 007           |
| Concurrency            | M:N tasks, channels, `async`/`await`; not 1:1 OS thread per task                                           | 003, 006                |
| Structured concurrency | Encouraged (`taskScope`); fire-and-forget `spawn` allowed + lintable                                       | 003                     |
| Data races             | Not silent UB; happens-before documented; race detector (dev)                                              | 003, 006                |
| Generics               | Monomorphization for concrete generics; interface dispatch via vtable                                      | 002, 004                |
| Surface style          | Statement-oriented + expression-capable `if`/`match`/blocks                                                | 001                     |
| Backend                | LLVM native codegen                                                                                        | 004                     |
| Toolchain impl         | Rust                                                                                                       | 004, 005, 008, 012, 013 |
| Deploy                 | Single executable by default; **static link runtime**                                                      | 008, 006, 013           |
| Interop v1             | C ABI FFI                                                                                                  | 006, 007                |
| Targets v1             | Server + CLI; linux/mac/win × amd64/arm64; no WASM day-one                                                 | 000, 013                |
| Macros v1              | Attributes + declarative macros; sandboxed proc plugins later                                              | 010                     |
| Packages               | `aura.toml` + lockfile + registry; **commit lockfiles always**                                             | 005                     |
| Build scripts          | None in MVP (declarative only)                                                                             | 008                     |
| Stability              | SemVer for toolchain/stdlib; language editions post-MVP                                                    | 000                     |

### 6.6 Guiding trade-offs

| Trade-off                        | Lean toward                           | Accept cost                                 |
| -------------------------------- | ------------------------------------- | ------------------------------------------- |
| Safety vs ceremony               | Null-safe types, explicit `?`         | Slightly more annotations than classic Java |
| Perf vs abstraction              | LLVM + mono generics + linked runtime | Longer cold compile than scripting          |
| Stability vs velocity            | Core small; editions later            | Fewer “change everything” releases          |
| Single binary vs dynamic plugins | Single binary default                 | Plugin model deferred / constrained         |
| Class OOP vs data-only           | Classes + interfaces                  | More complex than Go structs-only           |

### 6.7 Versioning & stability policy (high level)

- **Toolchain & stdlib:** Semantic Versioning. Breaking stdlib APIs require major bump; deprecations prefer two minors of warning when practical.
- **Language:** MVP freezes a “v1 surface.” Breaking surface changes after adoption prefer **editions** (opt-in per package) rather than silent breaks.
- **Packages:** Manifest declares Aura edition/language version range (details in RFC-005).

### 6.8 Examples

```aura
// Conceptual tour — syntax stabilized in RFC-001
package demo

class Server {
  fun start(port: Int): Result<Unit, IoError> {
    // bind + accept loop using stdlib net + tasks
    return Result.ok(Unit)
  }
}

fun main() {
  let s = Server()
  match s.start(8080) {
    case Ok(_) => println("listening")
    case Err(e) => throw e   // or log and exit
  }
}
```

End-to-end narrative: author writes the above → `aura new` / edit → `aura test` → `aura build -o server` → copy `server` to host → run. No separate GC/runtime install.

### 6.9 Error model / edge cases

N/A at vision level for program errors. Process-level edge cases:

- Conflict between “Java-like classes” and “Go-like tasks” is intentional; document identity/sharing under GC (RFC-003).
- Single-binary vs future dynamic plugins: plugins are non-goals for core v1.

### 6.10 Compatibility & migration

- **No JS/TS source compatibility.** Interop is C ABI and (later) well-defined binary interface for Aura packages.
- Migration from other languages is human/port, not automated transpile as a core promise.
- Within Aura: deprecation + editions as above.

## 7. Open questions

| #   | Question                                                       | Options                                         | Owner   | Status                                                                            |
| --- | -------------------------------------------------------------- | ----------------------------------------------- | ------- | --------------------------------------------------------------------------------- |
| 1   | Exact GC algorithm (Immix, Go-style, concurrent mark-sweep, …) | TBD in RFC-006                                  | Runtime | **Resolved** — phased: free-all MVP → STW mark-sweep → concurrent later (RFC-006) |
| 2   | Checked exceptions vs unchecked-only + Result                  | Unchecked + Result                              | Lang    | **Resolved** — unchecked + `Result`                                               |
| 3   | Value types / `struct` distinct from `class` in v1?            | Yes                                             | Lang    | **Resolved** — yes                                                                |
| 4   | Brand, license, governance                                     | Brand **Aura**; **MIT** license; governance TBD | Project | **Resolved** — MIT; governance lightweight/Deferred until community               |
| 5   | Release signing technology                                     | cosign / minisign / …                           | Dist    | **Resolved** — minisign first (align RFC-013); cosign optional later              |

## 8. Rationale & trade-offs

Choosing **GC + classes** optimizes for service developer productivity and a familiar mental model, accepting GC pauses and a richer runtime than pure ownership languages. Choosing **LLVM + single binary** optimizes for deploy simplicity and native performance, accepting heavier compile infrastructure than a bytecode-only story. Choosing **Rust for the toolchain** optimizes for toolchain reliability and ship speed without waiting for self-host. Rejecting frameworks in core keeps the RFC set implementable and avoids premature ecosystem lock-in.

## 9. Unresolved / future work

- Finalize brand, license, governance
- Logo, website, playground
- WASM target RFC (post-v1)
- Self-host roadmap
- Optional ownership/region annotations as opt-in performance layer (not v1 requirement)

## 10. Security & safety considerations

Security is a design axis, not an add-on:

| Area         | Stance                                                                      |
| ------------ | --------------------------------------------------------------------------- |
| Memory       | GC eliminates classic UAF/double-free in safe Aura; unsafe/FFI are explicit |
| Supply chain | Lockfiles, signed releases (RFC-013), registry policy (RFC-005)             |
| Sandbox      | Compiler plugins sandboxed when introduced (RFC-010)                        |
| Concurrency  | Race detector; document that races are bugs, not optimization license       |
| Injection    | No `eval` of Aura source in stdlib v1                                       |

## 11. Implementation plan (optional)

| Phase         | Scope                  | Exit criteria                |
| ------------- | ---------------------- | ---------------------------- |
| Vision freeze | RFC-000 content stable | Principles signed off        |
| Language MVP  | 001–003 solid          | Toy programs fully specified |
| Compiler MVP  | 004, 006, 012          | Compile & run hello          |
| Ship MVP      | 005, 007, 008, 011–013 | Build, test, ship one binary |

## 12. References

- RFC-001 … RFC-013 (this series)
- Go memory/concurrency model (inspiration)
- Kotlin nullability (inspiration)
- LLVM project
- Rust tooling patterns (Cargo-like workflow inspiration for Aura CLI)

---

## Changelog

| Date       | Author | Change                                                           |
| ---------- | ------ | ---------------------------------------------------------------- |
| 2026-07-16 |        | Lock remaining open Qs: phased GC, minisign, governance Deferred |
| 2026-07-15 |        | **Accepted**; MIT license; execution via docs/roadmap            |
| 2026-07-15 |        | Initial skeleton                                                 |
| 2026-07-15 |        | Solid draft: locked decisions, principles, non-goals             |
| 2026-07-15 |        | Promote In Review; lock lean language/deploy decisions           |
