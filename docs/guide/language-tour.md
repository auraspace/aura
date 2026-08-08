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

## Recommended learning path

Read the language guides in this order:

1. [Types & nullability](./types-and-nullability.md) — values, null safety,
   inference, and generics basics.
2. [Classes, structs & interfaces](./classes-and-structs.md) — the object
   model, inheritance, overriding, visibility, and value types.
3. [Control flow & errors](./control-flow-and-errors.md) — branches, loops,
   enums, `match`, `Result`, and exceptions.
4. [Arrays](./arrays.md) — collections, ownership, iteration, and higher-order
   functions.
5. [Async, tasks & borrowing](./async-and-borrowing.md) — `async`, `await`,
   tasks, channels, cancellation, and scoped `ref` values.
6. [Attributes & derives](./attributes-and-derives.md) — tests, derives, FFI,
   and declaration metadata.
7. [Syntax cheatsheet](./syntax-cheatsheet.md) — compact reference while
   writing programs.

After the language core, continue with [Standard library](./standard-library.md),
[Packages](./packages.md), and [Testing](./testing.md).

## Topic guides

| Guide                                                     | What you learn                                         |
| --------------------------------------------------------- | ------------------------------------------------------ |
| [Types & nullability](./types-and-nullability.md)         | Scalars, `T` vs `T?`, narrowing, generics basics       |
| [Classes, structs & interfaces](./classes-and-structs.md) | Classes, inheritance, override, visibility, OOP        |
| [Control flow & errors](./control-flow-and-errors.md)     | `if`/`for`/`match`, `Result`, throw/catch              |
| [Arrays](./arrays.md)                                     | `Array<T>`, ownership, iteration, and HOFs             |
| [Async, tasks & borrowing](./async-and-borrowing.md)      | `async`, `await`, tasks, channels, cancellation, `ref` |
| [Attributes & derives](./attributes-and-derives.md)       | `@test`, derives, metadata, and FFI declarations       |
| [Syntax cheatsheet](./syntax-cheatsheet.md)               | Compact lookup for syntax and common APIs              |
| [Standard library](./standard-library.md)                 | All shipped `std.*` packages and API contracts         |

## What works in the compiler today

These topics match **in-tree** behavior (corpus + CLI), not only Accepted RFCs:

- Packages, functions, locals, expressions
- Nullability flow, force-unwrap `!!`, coalesce `?:`, safe call `?.`
- Classes (GC), single inheritance, `open`/`final`/`abstract`, `override`,
  visibility, structs (value), interfaces (`class C : I`), and monomorphized
  generics (including generic interface/class implements)
- Enums + `match`, `Result`
- `throw` / `try` / `catch` / `finally`; `if` as expression
- `async fun`, `await`, `spawn`, `join`, `cancel`, and bounded `Channel<T>` operations
- Scoped non-owning `ref T` values with lexical escape and async-boundary checks
- Declaration attributes: `@test`, `@bench`, `@derive`, `@deprecated`, `@foreign`,
  `@repr`, `@reflect`, `@notNull`, and optimization/error metadata
- `Array<T>` (+ `clone`, nested free), ranges, `for-in` (array / string bytes / Iterable)
- String `+`, `"hi ${name}"` interpolation (idents), `substring(start, end)` (exclusive end; UTF-8 **byte** indices)
- Other String helpers: `len`, `isEmpty`, `charAt`, `startsWith` / `contains` / `endsWith`, `indexOf`, `split`, `trim` / `trimStart` / `trimEnd`, `toInt(): Int?`
- `type` aliases, top-level `const`, `is` type test
- Expression-body functions `fun f(): T = expr`
- First-class functions / lambdas: `(x: T) => expr`, block body, fun type `(T) -> U`
- Captures: outer `val` of `Int` / `Bool` / `String` / class / Array / Fun; outer `var` of scalar, String, class, Array, or Fun values through shared boxes (C20c–e)
- Multi-file packages, imports, path deps; `aura new` / `init` / `version`
- `aura run` / `test` pass-through after `--`; `aura test` + `@test`
- Shipped `std.*` packages, including `std.io` console/file/argv/stdin/exit,
  `std.collections`, typed errors, async networking, timers, tasks, and tests
- Generic `std.collections` Map/Set/HashMap/HashSet/Iterable, snapshots/live views,
  entry handles, generic HOFs, and `join`
- Dogfood CLI: `examples/wc` (args + String tools)

## Still design-first (limited or deferred in code)

- General lambda capture/control-flow combinations beyond the covered ownership fixtures — see repo debts
- General **task / async** lowering and all outcome/IO shapes remain limited;
  bounded await/spawn/channel slices ship ([RFC-003](/rfc/003), [RFC-006](/rfc/006))
- Macros / plugins ([RFC-010](/rfc/010))
- Reflection ([RFC-009](/rfc/009))
- LLVM backend as default ([RFC-004](/rfc/004) — C remains the default compatibility backend; LLVM is available for complete scalar-MIR programs)
- Verified locked origin consumption: HTTPS metadata/archive fetch, direct Git
  tag/revision resolution, semver pinning, SHA-256 verification, cache
  extraction, and offline warm-cache builds. The Go-module-inspired origin
  publication workflow and `git=`/`github=` sources are supported; workspaces
  and proxy serving are intentionally later work.
- Live collection iterators are invalidation-checked by collection epoch;
  snapshot iterators remain available when mutation-safe copies are preferred

See the [roadmap map](./roadmap.md#rfc-accepted-vs-implemented) for a per-RFC table.

## Next

- [Getting started](./getting-started.md) if you have not run hello yet
- [Types & nullability](./types-and-nullability.md)
- [Classes, structs & interfaces](./classes-and-structs.md) for the OOP model
- [Syntax cheatsheet](./syntax-cheatsheet.md) when you need a quick lookup
