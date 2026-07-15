# Aura roadmap

Living plan for docs, language specs, and the Rust toolchain. RFCs remain the design source of truth; this file tracks **execution order**.

| Field | Value |
| ----- | ----- |
| **Updated** | 2026-07-15 |
| **Strategy** | Dual-track: freeze MVP surface in RFCs while shipping vertical compiler slices |
| **License** | MIT (see root `LICENSE`) |

## Status snapshot

| Track | Status |
| ----- | ------ |
| RFC static site (`site/`) | Implemented on `feat/rfc-static-site`; deploy via GitHub Pages Actions |
| RFC-000 Vision | **Accepted** — product direction locked |
| RFC-001/002/003 | Solid Draft + **MVP subset** for compiler C0–C1 (see RFC-001 §6.0) |
| Compiler | **C0+ done** (check); **C1 done** via C backend (`aura build`/`run`) |
| Runtime / packages / stdlib | Stub runtime `runtime/aura_rt.c`; full GC/tasks deferred |

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
| 1.1 | Accept RFC-000 | Status **Accepted** |
| 1.2 | Freeze RFC-001 MVP surface (G0/G1) | §6.0 + keyword list v0 |
| 1.3 | RFC-002 subset for nullability + nominal basics | Enough for `aura check` on corpus |
| 1.4 | RFC-003: declare GC/tasks; defer runtime depth | Explicit “not in C0/C1” |
| 1.5 | Corpus under `corpus/` | ≥10 programs; C0 parses them |

**Out of scope for P1 depth:** full 005–013, macros (010), reflection (009).

### P2 — Compiler bootstrap (C0 → C1)

Rust workspace (toolchain only; user language remains Aura):

| Milestone | Scope | Exit |
| --------- | ----- | ---- |
| **C0** | Lexer + recursive-descent parser + `aura check` | Done |
| **C0+** | Name resolution + light typecheck | Done (`aura check` typechecks corpus) |
| **C1** | Codegen + runtime stub → native binary | Done via **C backend** + `cc` (LLVM later) |
| **C1b** | Simple `class` + methods + local nullability | Next |

**Out of scope C0/C1:** generics mono, async/tasks, macros, registry, incremental, LTO.

### P3 — Expand (after hello)

1. Generics → interfaces/vtable → `Result`/exceptions → `struct`
2. Runtime: alloc/GC MVP → channels/tasks
3. Toolchain: `aura.toml` → build graph → `aura test`
4. Stdlib prelude + small collections
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
