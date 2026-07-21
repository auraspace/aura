---
title: Language tour
section: Language
order: 30
summary: Map of the language surface — start here, then dive into each topic.
---

# Language tour

This is the **index** for the language guides. For normative rules, always prefer [RFC-001](/rfc/001) and [RFC-002](/rfc/002).

## Hello shape

```aura
package main

fun main() {
  println("Hello, Aura")
}
```

Every file lives in a **package**. Programs enter at `fun main()`.

## Topic guides

| Guide                                                     | What you learn                                     |
| --------------------------------------------------------- | -------------------------------------------------- |
| [Types & nullability](./types-and-nullability.md)         | Scalars, `T` vs `T?`, flow narrowing               |
| [Classes, structs & interfaces](./classes-and-structs.md) | Reference vs value types, generics                 |
| [Control flow & errors](./control-flow-and-errors.md)     | `if`/`for`/`match`, `Result`, throw/catch          |
| [Arrays](./arrays.md)                                     | `Array<T>`, push/pop, iteration, Int HOF helpers   |
| [Syntax cheatsheet](./syntax-cheatsheet.md)               | Compact lookup (incl. lambdas / fun types)         |
| [Standard library](./standard-library.md)                 | `std.io`, `std.assert`, `std.collections`, prelude |

## What works in the compiler today

These topics match **in-tree** behavior (corpus + CLI), not only Accepted RFCs:

- Packages, functions, locals, expressions
- Nullability flow, force-unwrap `!!`, coalesce `?:`, safe call `?.`
- Classes (GC), structs (value), interfaces (`class C : I`), monomorphized generics (incl. generic iface/class implements)
- Enums + `match`, `Result`
- `throw` / `try` / `catch` / `finally`; `if` as expression
- `Array<T>` (+ `clone`, nested free), ranges, `for-in` (array / string bytes / Iterable)
- String `+`, `"hi ${name}"` interpolation (idents), `substring(start, end)` (exclusive end; UTF-8 **byte** indices)
- Other String helpers: `len`, `isEmpty`, `charAt`, `startsWith` / `contains` / `endsWith`, `indexOf`, `split`
- `type` aliases, top-level `const`, `is` type test
- Expression-body functions `fun f(): T = expr`
- First-class functions / lambdas: `(x: T) => expr`, block body, fun type `(T) -> U`
- Captures MVP: outer immutable `val` of `Int` / `Bool` / `String` only (no `var`, class, Array, or nested Fun yet)
- Multi-file packages, imports, path deps; `aura new` / `init` / `version`
- `aura test` + `@test`
- `std.io` console + file I/O; `std.assert`; `std.collections` Map/Set/HashMap/Iterable + Int HOF

## Still design-first (limited or deferred in code)

- Richer **lambda captures** (class / Array / env GC) — see repo debts
- Full **task / async** surface ([RFC-003](/rfc/003), [RFC-006](/rfc/006))
- Macros / plugins ([RFC-010](/rfc/010))
- Reflection ([RFC-009](/rfc/009))
- LLVM backend as default ([RFC-004](/rfc/004) — C backend is what runs now)
- Registry fetch / semver ([RFC-005](/rfc/005) — path deps + lock schema only)

See the [roadmap map](./roadmap.md#rfc-accepted-vs-implemented) for a per-RFC table.

## Next

1. [Getting started](./getting-started.md) if you have not run hello yet
2. [Types & nullability](./types-and-nullability.md)
3. [Syntax cheatsheet](./syntax-cheatsheet.md) when you need a quick lookup
