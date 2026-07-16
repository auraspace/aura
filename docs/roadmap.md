# Aura roadmap

Living plan for docs, language specs, and the Rust toolchain. RFCs remain the design source of truth; this file tracks **execution order**.

| Field | Value |
| ----- | ----- |
| **Updated** | 2026-07-16 |
| **Strategy** | Dual-track: freeze MVP surface in RFCs while shipping vertical compiler slices |
| **License** | MIT (see root `LICENSE`) |

## Status snapshot

| Track | Status |
| ----- | ------ |
| RFC static site (`site/`) | Implemented on `feat/rfc-static-site`; deploy via GitHub Pages Actions |
| RFC-000 … RFC-013 | **All Accepted** — open questions resolved or Deferred (2026-07-16) |
| Language MVP | RFC-001 §6.0 + post-C1 surface in roadmap; async/macros/Iterable still deferred in code |
| Compiler | **C0–C4t done** — through if-expression, `?.`, Array/String APIs |
| Runtime / packages / stdlib | GC free-all MVP + `std.io` / `std.assert`; next GC = STW mark-sweep; tasks/collections deferred |

## Phases

```text
[P0] Ship docs site  →  [P1] Spec MVP  →  [P2] Compiler hello  →  [P3] Expand
```

### P0 — Docs site

- Parse and render RFCs from `docs/rfc/`
- Prerender static HTML; selective hydration (search, filter, theme, graph)
- Deploy to GitHub Pages (`VITE_BASE=/aura/`)

**Exit:** public URL + green deploy workflow.

### P1 — Spec MVP (Wave 1 content)

| Step | Work | Exit |
| ---- | ---- | ---- |
| 1.1 | Accept RFC-000 | Status **Accepted** ✓ |
| 1.2 | Freeze RFC-001 MVP surface (G0/G1) | §6.0 + keyword list v0 ✓ |
| 1.3 | RFC-002 subset for nullability + nominal basics | Enough for `aura check` on corpus ✓ |
| 1.4 | RFC-003: declare GC/tasks; defer runtime depth | Explicit “not in C0/C1” ✓ |
| 1.5 | Corpus under `corpus/` | ≥10 programs; C0 parses them ✓ |
| 1.6 | Status review + lock open Qs (2026-07-16) | All RFC-000…013 **Accepted**; remaining items Deferred or phased ✓ |

**Out of scope for P1 depth:** full 005–013, macros (010), reflection (009).

### P2 — Compiler bootstrap (C0 → C1)

Rust workspace (toolchain only; user language remains Aura):

| Milestone | Scope | Exit |
| --------- | ----- | ---- |
| **C0** | Lexer + recursive-descent parser + `aura check` | Done |
| **C0+** | Name resolution + light typecheck | Done (`aura check` typechecks corpus) |
| **C1** | Codegen + runtime stub → native binary | Done via **C backend** + `cc` (LLVM later) |
| **C1b** | Simple `class` + methods + `this` + field access | Done |
| **C2a** | `interface` + implements + closed-world dispatch | Done |
| **C2b** | Generics (class/fun type params + mono) | Done |
| **C2c** | Type-arg inference from args + expected type | Done |
| **C2d** | Nullability flow (`if (x != null)`) + `!!` | Done |
| **C2e** | Multi-param bounds / `where` clauses | Done |
| **C3a** | `struct` value types (fields + methods; no implements) | Done |
| **C3b** | `enum` + `match` + generic `Result<T,E>` (exhaustive) | Done |
| **C3c** | `throw` / `try` / `catch` / `finally` (String/Int/Bool) | Done |
| **C3d** | `aura test` + `@test` + `assert` / `assert_eq` | Done |
| **C3e** | Multi-file same package + minimal `aura.toml` | Done |
| **C3f** | `import` + `pub` + path deps in `aura.toml` | Done |
| **C3g** | `throw` / `catch` class & struct values | Done |
| **C3h** | `for (i in start..end)` exclusive Int range | Done |
| **C3i** | `break` / `continue` in loops | Done |
| **C3j** | Builtin `Array<T>` (`T` = Int/Bool/String; len/get/set) | Done |
| **C3k** | `for (x in array)` over `Array<T>` | Done |
| **C3l** | Inclusive range `for (i in a..=b)` | Done |
| **C3m** | `Array.push` + capacity grow | Done |
| **C3n** | `import path as Alias` + `Alias.fun(...)` | Done |
| **C3o** | Package-prefixed free-function C symbols | Done |
| **C3p** | `aura.lock` for path deps (verify + write) | Done |
| **C3q** | Bare comparison emit (no double parens / `-Wparentheses-equality`) | Done |
| **C3r** | `Array.pop` (empty throws) | Done |
| **C3s** | Free `throw_obj` payloads on `aura_ex_clear` | Done |
| **C3t** | Free owned `Array` heap buffers at scope end / return | Done |
| **C3u** | `import … as Alias` type qualify (`Alias.Type` / ctor) | Done |
| **C3v** | Package-prefixed class/enum C symbols + multi-key tables | Done |
| **C3w** | `for-in` over String (UTF-8 bytes as Int) | Done |
| **C3x** | GC MVP: `aura_gc_alloc` + shutdown free-all | Done |
| **C3y** | Class instances as GC heap references (`struct` by-value) | Done |
| **C3z** | Minimal `std.io` package (`println` via path dep) | Done |
| **C4a** | Class identity `==` / `!=` (pointer; corpus) | Done |
| **C4b** | Nullable class `Class?` C emit + null flow | Done |
| **C4c** | `Array` of class heap references | Done |
| **C4d** | Package-prefixed interface C symbols + multi-key | Done |
| **C4e** | String content equality (`strcmp`) | Done |
| **C4f** | `Array.clear` (len=0, keep cap) | Done |
| **C4g** | Auto-prelude `std.io` for packages | Done |
| **C4h** | `std.assert` package + auto path for `std.*` | Done |
| **C4i** | Reject struct/enum/interface `==` in sema | Done |
| **C4j** | Nested path deps recorded in `aura.lock` | Done |
| **C4k** | Type-param heap class pointers + field method recv | Done |
| **C4l** | `else if` chaining (desugar to nested if) | Done |
| **C4m** | Null coalesce `?:` | Done |
| **C4n** | `Array.isEmpty` | Done |
| **C4o** | `Array.reserve(n)` (grow cap, keep len) | Done |
| **C4p** | `String.len` (UTF-8 byte length) | Done |
| **C4q** | `Array` of struct by-value elements | Done |
| **C4r** | Free Array buffer on owner reassignment | Done |
| **C4s** | Safe call `?.` on nullable receivers | Done |
| **C4t** | `if` as expression (last branch expr; requires else) | Done |
| **DX** | line:col diagnostics with snippets | Done |

**Out of scope C0/C1:** generics mono, async/tasks, macros, registry, incremental, LTO.

### P3 — Expand (after hello)

1. ~~Language surface C2–C4t~~ → next: Iterable · LLVM · registry (see `agents/debts.md`)
2. Runtime: ~~alloc/GC MVP + class heap refs~~ → channels/tasks
3. Toolchain: ~~minimal `aura.toml` + path deps + path lock (incl. transitive)~~ → registry (RFC-005)
4. Stdlib: ~~std.io + auto-prelude + std.assert~~ → collections (partial: builtin Array)
5. Cross targets + signed releases

Write Wave 2–4 RFCs **as implementation needs them**, not all up front.

## Sprint discipline

Each sprint should ship:

1. One slice of **surface** (RFC amend or open-question resolve/defer)
2. One slice of **compiler** (tests green)
3. Corpus update when syntax changes

## Definition of done

| Phase | Done when |
| ----- | --------- |
| P0 | Pages URL + workflow green |
| P1 | 000 Accepted; 001/002 MVP subset; corpus ≥10; open Q resolved or deferred |
| P2 | `aura build` produces a running hello binary |
| P3 | Feature-sized follow-ups (not one mega-PR) |

## Non-goals (near term)

- Full 80–120 page RFC-001 before a parser exists
- Package registry, macros, race detector before hello
- Self-hosting the compiler in Aura
- Site i18n / in-browser RFC editing

## Related docs

- [RFC index](rfc/README.md)
- [RFC-000 Vision](rfc/RFC-000-vision-design-principles.md)
- [RFC-001 Language](rfc/RFC-001-language-specification.md) — MVP §6.0
- [RFC-004 Compiler](rfc/RFC-004-compiler-architecture.md)
- [Site README](../site/README.md)
