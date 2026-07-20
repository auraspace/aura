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

| Guide                                                     | What you learn                            |
| --------------------------------------------------------- | ----------------------------------------- |
| [Types & nullability](./types-and-nullability.md)         | Scalars, `T` vs `T?`, flow narrowing      |
| [Classes, structs & interfaces](./classes-and-structs.md) | Reference vs value types, generics        |
| [Control flow & errors](./control-flow-and-errors.md)     | `if`/`for`/`match`, `Result`, throw/catch |
| [Arrays](./arrays.md)                                     | `Array<T>`, push/pop, iteration           |
| [Syntax cheatsheet](./syntax-cheatsheet.md)               | Compact lookup for keywords and forms     |
| [Standard library](./standard-library.md)                 | `std.io`, `std.assert`, prelude           |

## What works in the compiler today

These topics match **in-tree** behavior (corpus + CLI), not only Accepted RFCs:

- Packages, functions, locals, expressions
- Nullability flow and force-unwrap
- Classes (GC), structs (value), interfaces, monomorphized generics
- Enums + `match`, `Result`
- `throw` / `try` / `catch` / `finally`
- `Array<T>`, ranges, `for-in`
- Multi-file packages, imports, path deps
- `aura test` + `@test`

## Still design-first (limited or deferred in code)

- Full **task / async** surface ([RFC-003](/rfc/003), [RFC-006](/rfc/006))
- Macros / plugins ([RFC-010](/rfc/010))
- Reflection ([RFC-009](/rfc/009))
- LLVM backend as default ([RFC-004](/rfc/004) — C backend is what runs now)

See the [roadmap map](./roadmap.md#rfc-accepted-vs-implemented) for a per-RFC table.

## Next

1. [Getting started](./getting-started.md) if you have not run hello yet
2. [Types & nullability](./types-and-nullability.md)
3. [Syntax cheatsheet](./syntax-cheatsheet.md) when you need a quick lookup
