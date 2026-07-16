---
title: Standard library
section: Toolchain
order: 55
summary: In-tree std packages — std.io, std.assert, and how prelude resolution works.
---

# Standard library

Aura’s **core** stdlib is intentionally small ([RFC-007](/rfc/007), [RFC-000](/rfc/000) batteries-included-but-modular). In this repository, packages live under `std/`.

## Packages today

| Package | Path | Role |
| ------- | ---- | ---- |
| `std.io` | `std/io` | Printing / basic I/O (`println`) |
| `std.assert` | `std/assert` | Assert helpers for tests |
| `std.collections` | `std/collections` | Placeholder / evolving — check package README |

Builtins such as `Array<T>` and core scalars are part of the **language**, not a separate import.

## `std.io`

Typical use (explicit import or auto-prelude on package builds):

```aura
package main

// Often available via auto-prelude on package builds
fun main() {
  println("Hello, Aura")
}
```

Corpus:

```bash
cargo run -p aura-cli -- run corpus/std_io/app
cargo run -p aura-cli -- run corpus/std_io/prelude
```

## `std.assert`

Use with `aura test` and `@test` functions:

```bash
cargo run -p aura-cli -- run corpus/std_assert/app
```

Prefer package tests that exercise `assert` / `assert_eq` for `Int` / `String` / `Bool` in the current MVP.

## How the CLI finds `std.*`

Milestones in the root README (C4g / C4h):

- Auto-prelude **`std.io`** for package builds
- Path resolution for `std.*` imports via `AURA_STD` or walk-up from the package

If imports fail, verify you are invoking the CLI on a **package directory** (with `aura.toml`) and that `std/` is reachable from the monorepo layout.

## What is *not* in core (by design)

Application frameworks, DI containers, ORM/HTTP stacks stay **out of core** RFCs. Expect those as ecosystem packages later, not as stdlib defaults.

## Next

- [Packages](./packages.md)
- [Testing](./testing.md)
- [RFC-007](/rfc/007)
