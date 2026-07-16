# Aura — Documentation

Design documents and specifications for the **Aura** language core and toolchain (compiler, runtime, package manager, build, CLI).

## Layout

```
docs/
├── README.md          ← you are here
├── roadmap.md         ← execution phases (P0–P3, compiler C0–C4t+)
├── rfc/
│   ├── README.md      ← RFC index, status, dependencies
│   ├── TEMPLATE.md    ← required template for every new RFC
│   ├── RFC-000-….md
│   ├── …
│   └── RFC-013-….md
└── superpowers/       ← session plans/specs (not product RFCs)
    ├── plans/
    └── specs/
```

## Where to start

| If you want…                     | Read…                                              |
| -------------------------------- | -------------------------------------------------- |
| Execution roadmap (phases)       | [roadmap.md](roadmap.md)                           |
| Vision & design principles       | [RFC-000](rfc/RFC-000-vision-design-principles.md) |
| Syntax & core language semantics | [RFC-001](rfc/RFC-001-language-specification.md)   |
| MVP surface for compiler C0–C1   | [RFC-001 §6.0](rfc/RFC-001-language-specification.md#60-mvp-surface-compiler-c0c1) |
| Type system                      | [RFC-002](rfc/RFC-002-type-system.md)              |
| Memory model & concurrency       | [RFC-003](rfc/RFC-003-memory-model-concurrency.md) |
| Full RFC catalog                 | [rfc/README.md](rfc/README.md)                     |

## Scope

**In scope:** language, types, memory/concurrency, compiler (Rust), runtime, stdlib, packages, build, test, CLI, binary distribution.

**Out of scope for now:** web/application frameworks, DI containers, ORM/data layers.

## Conventions

1. Every new RFC **must** be copied from [`rfc/TEMPLATE.md`](rfc/TEMPLATE.md).
2. RFC numbers are stable; titles may change, numbers **must not**.
3. Valid statuses: `Draft` → `In Review` → `Accepted` → `Frozen` (or `Rejected` / `Superseded`).
4. Breaking changes after `Accepted` require an amendment RFC or a new RFC that references the original.
5. Page estimates are **depth targets**, not hard caps.

## Documentation language

- **English** for all narrative, rationale, and design decisions.
- Technical identifiers, API surfaces, keywords, error codes, and paths remain as in code.
- Code samples use **Aura** (or pseudo-Aura when syntax is unstable; call that out explicitly).
