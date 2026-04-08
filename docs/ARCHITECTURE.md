# Aura Architecture (Compiler + Runtime)

Aura is an OOP, statically typed language with a TypeScript-like surface syntax and a Go-like distribution model: `aurac` compiles `.aura` / `.ar` sources into a single native binary that can embed a small runtime (memory management, strings, arrays, panic, etc.).

This document describes a minimal but scalable architecture for the Aura compiler and runtime. The initial implementation focuses on `aarch64-apple-darwin`.

## Goals

- **Single-binary output**: one executable per build (no external VM required).
- **Embeddable runtime**: runtime ships as a static library linked into the final binary.
- **Fast iteration**: clean separation between frontend (syntax/types) and backend (codegen/link).
- **Multi-target ready**: target-specific pieces are isolated behind clear interfaces.
- **TS-like syntax, OOP semantics**: classes, interfaces, methods, and dynamic dispatch where needed.

## Non-Goals (Initial Milestone)

- Full TypeScript compatibility.
- JIT or bytecode VM.
- Complete standard library.
- Whole-program LTO and advanced optimizations (can be added later).

## High-Level Pipeline

1. **Parse**: source text -> tokens -> AST
2. **Resolve**: names, imports, symbol tables, type names -> resolved AST (or HIR)
3. **Type check**: annotate expressions and declarations with concrete types
4. **Lower**: typed AST/HIR -> MIR (typed, explicit control flow, explicit calls)
5. **Codegen**: MIR -> object file (`.o`) for the selected target
6. **Link**: object + runtime staticlib + system libs -> final executable

## Architecture Diagram

```mermaid
flowchart LR
  subgraph "Source Inputs"
    A[".aura / .ar files"]
    B["Build config (target, flags)"]
  end

  subgraph "Compiler Frontend"
    L["Lexer"] --> P["Parser (AST)"]
    P --> R["Resolver (symbols/imports)"]
    R --> T["Type Checker (typed AST/HIR)"]
  end

  subgraph "Middle-End"
    T --> LO["Lowering (HIR -> MIR)"]
    LO --> MIR["MIR (typed CFG)"]
  end

  subgraph "Backend"
    MIR --> CG["Codegen (Cranelift/LLVM)"]
    CG --> OBJ["Object file (.o)"]
    OBJ --> LK["Linker (clang/ld)"]
  end

  subgraph "Embedded Runtime"
    RT["libaura_rt.a\n(alloc/string/array/exceptions/panic)"]
  end

  A --> L
  B --> R
  B --> CG
  RT --> LK
  LK --> EXE["Single native executable"]
```

```mermaid
flowchart TB
  subgraph "Runtime (libaura_rt.a)"
    M["Memory (alloc, ARC/GC hooks)"]
    S["String"]
    V["Array/Vector"]
    EH["Exceptions (try/catch/finally)\nhandler frames + unwind"]
    PN["Panic/Trap"]
  end

  EH --> PN
  S --> M
  V --> M
```

The compiler should support debug/introspection modes:

- `--emit=ast|hir|mir|obj|asm`
- `--print=types|symbols|imports`

## Compiler Components

### Frontend

**Lexer**

- Produces tokens with trivia (comments/whitespace) for diagnostics and tooling.
- Keeps source spans for all tokens.

**Parser**

- Produces an AST closely matching the surface syntax (TS-like).
- Error recovery should be present early (simple "sync points" on `;`, `}`, `)`).

**Diagnostics**

- Centralized diagnostic type: span + message + optional fix-it hints.
- Stable error codes (useful for editor integration later).

**Module/Package Loader**

- File-based modules: resolves `import ... from "./path"` to files.
- Build root and module roots are explicit CLI options (no hidden magic).

**Name Resolution**

- Builds symbol tables per module.
- Resolves:
  - local bindings
  - type names
  - member lookup
  - imports/exports

**Type System (Minimal)**

- Primitive types (`i32`, `i64`, `f32`, `f64`, `bool`, `string`, `void`).
- Nominal class types.
- Nominal interfaces (initially), with `implements` checks.
- Generics can start as "monomorphize at use sites" (no runtime generics initially).

### Middle-End IR

Use a staged IR design to keep the compiler maintainable:

- **HIR** (optional early): a syntax-lean representation that normalizes sugar (e.g., `for` -> `while`).
- **MIR** (recommended): typed, explicit temporaries, explicit control flow graph (CFG).

MIR should make these explicit:

- control flow blocks + terminators (branch, return, trap)
- calls (direct and virtual)
- object allocation and field access
- string/array operations (lower to runtime calls)

This makes the backend simpler and keeps "language semantics" largely in the frontend/middle-end.

### Backend

The backend is responsible for:

- target selection (`triple`, `cpu`, `features`, pointer size, calling convention)
- object emission
- linking

For the initial milestone on `aarch64-apple-darwin`, a practical approach is:

- Codegen to Mach-O object (`.o`)
- Link via the system toolchain (`clang` / `ld`) into a single executable

Backend should be pluggable:

- `Backend::compile(mir, out_dir) -> ObjectFilePath`
- `Backend::emit_llvm(...)` / `Backend::emit_asm(...)` for debug outputs
- `Linker::link(objects, runtime, target) -> Executable`

Implementation options:

- **LLVM**: MVP backend and the currently implemented backend in this repository.
- **Cranelift**: planned backend; may exist as a placeholder crate before implementation lands.

Keep the abstraction so switching/adding backends is possible. For the current codebase, do not assume Cranelift is available beyond the placeholder wiring, and treat LLVM as the default backend for MVP builds.

## Runtime Architecture (Embedded)

Aura programs link a runtime static library (e.g. `libaura_rt.a`) into the final executable.

### Responsibilities

- Heap allocation for reference types (class instances, strings, arrays).
- Memory management strategy (choose one for MVP):
  - **ARC (reference counting)**: simpler, deterministic; needs cycle strategy (later) or "no cycles" guidance.
  - **Tracing GC**: more complex; can come later.
- String representation + basic operations.
- Array/vector representation.
- Exception support (throw/try/catch/finally) and unwinding.
- Panic/trap, stack traces (optional in MVP), and exit codes.
- (Later) reflection/RTTI, exceptions, async runtime, etc.

### ABI Between Generated Code and Runtime

Use a stable C ABI boundary:

- Runtime exports `extern "C"` functions with a documented signature.
- Generated code calls these functions directly.

Example runtime entry points (illustrative):

- `aura_alloc(size, align) -> *mut u8`
- `aura_retain(ptr)` / `aura_release(ptr)` (if ARC)
- `aura_string_new_utf8(ptr, len) -> AuraString*`
- `aura_panic(msg_ptr, msg_len) -> !`

This keeps codegen and runtime evolvable independently.

### Object Layout + Dispatch

For OOP and `extends`:

- Each class instance is a heap object with a header:
  - pointer to vtable (or type descriptor)
  - reference count / GC header (depending on strategy)
- Fields follow in a defined order.

Dynamic dispatch:

- Methods that can be overridden are invoked through the vtable.
- The compiler generates vtables at compile-time (whole-program) for the executable.

Interfaces:

- For MVP, treat interfaces as nominal and lower both class and interface method calls through the same whole-program vtable layout.
- The compiler assigns each method name a stable slot and stores a class-specific vtable pointer in the object header.
- Interface-typed values still use the same object pointer at runtime; the receiver type selects the method signature at call sites while the vtable slot supplies the implementation.

## Exceptions and Unwinding (Go-like, MVP-Friendly)

Aura needs `throw`, `try/catch/finally` with predictable semantics, but early compiler backends (especially non-LLVM) may not provide full "zero-cost" exception handling.

For the MVP, design exceptions as **language-level unwinding implemented by the Aura runtime**, similar in spirit to Go's `panic` + stack unwinding of defers:

- `throw <obj>` raises an exception object.
- The runtime unwinds stack frames until it finds a matching `catch` handler.
- During unwind, the compiler-emitted cleanup actions run (including `finally` blocks and implicit releases for ARC locals).

This approach:

- works without relying on platform-specific C++/DWARF EH tables
- provides deterministic `finally` execution
- integrates cleanly with ARC by treating releases as compiler-emitted cleanups

### Runtime Data Structures

Per-thread, maintain:

- a pointer to the current "handler frame" (linked list)
- the currently thrown exception object (or pointer)

Each handler frame contains:

- link to previous frame
- saved `setjmp` state for the `catch` entry
- cleanup stack pointer for `finally` and compiler-inserted cleanups

### Compiler Lowering Strategy

Frontend lowers `try/catch/finally` to MIR with explicit regions:

- **try region**: pushes a handler frame before executing try-body
- **catch entry**: receives the exception object, does type check, executes handler
- **finally cleanup**: executes on all exits (normal, return, throw)

Important: do not rely on OS-level unwinding to run destructors. Instead, the compiler should insert cleanup code explicitly and ensure it runs on both normal and exceptional edges.

### Backend Integration

Generated code interacts with the runtime via a small C ABI:

- `aura_try_begin(frame_ptr)` / `aura_try_end(frame_ptr)`
- `aura_throw(ex_ptr) -> !`
- `aura_current_exception() -> *mut AuraObject` (optional convenience)

The active `setjmp` site lives in the generated try dispatch or helper layer; `aura_throw` longjmps to the current handler frame, and `aura_try_begin`/`aura_try_end` maintain the per-thread handler list.

### Interop Rules (MVP)

- Exceptions **must not** cross the boundary into foreign (C) code unless explicitly wrapped.
- If an exception escapes `main`, the runtime prints a message and exits non-zero.

Future improvement path:

- Support platform zero-cost EH (LLVM Itanium/DWARF on macOS) under a feature flag.
- Add `throws` annotations and checked exceptions if desired (optional).

## Build + Tooling

### CLI Tooling

- `aurac build` (compile + link)
- `aurac run` (build + run)
- `aurac check` (parse + typecheck only)

### Reproducibility

- Deterministic compilation outputs where feasible.
- Emit a build manifest (inputs, target triple, runtime version).

## Milestones (Suggested)

1. Parse + diagnostics + `aurac check`
2. Type checker for primitives + functions
3. Minimal codegen: `main()` + integer arithmetic + printing (via runtime)
4. Classes + heap allocation + method calls
5. Inheritance + dynamic dispatch
6. Interfaces + `implements`
