---
title: Standard library
section: Toolchain
order: 55
summary: In-tree std packages — std.io, std.assert, std.collections, and prelude resolution.
---

# Standard library

Aura’s **core** stdlib is intentionally small ([RFC-007](/rfc/007), [RFC-000](/rfc/000) batteries-included-but-modular). In this repository, packages live under `std/`.

## Packages today (0.1.0-alpha)

| Package           | Path              | Role                                                  |
| ----------------- | ----------------- | ----------------------------------------------------- |
| `std.io`          | `std/io`          | Console + file I/O (`print`/`println`, `readFile`, …) |
| `std.assert`      | `std/assert`      | Assert helpers for tests                              |
| `std.collections` | `std/collections` | Map/Set/HashMap, Iterable, Int HOF helpers (C10i)     |

Builtins such as `Array<T>` and core scalars are part of the **language**, not a separate import.

## `std.io`

Console and file helpers (runtime `aura_*` intrinsics). File APIs throw a `String` message on failure (missing path, I/O error, oversized file, embedded NUL). Text is treated as a regular-file UTF-8 byte sequence (no embedded NUL); max size **256 MiB**.

| API                         | Role                               |
| --------------------------- | ---------------------------------- |
| `print` / `println`         | stdout (no newline / with newline) |
| `eprint` / `eprintln`       | stderr                             |
| `readFile(path): String`    | read entire regular file           |
| `writeFile(path, content)`  | create/truncate and write          |
| `appendFile(path, content)` | append (create if needed)          |
| `fileExists(path): Bool`    | regular file present               |
| `fileSize(path): Int`       | byte size (throws if missing)      |

Typical use (explicit import or auto-prelude on package builds):

```aura
package main

import std.io as Io

fun main() {
  Io.println("Hello, Aura")
  Io.writeFile("out.txt", "hi")
  val s = Io.readFile("out.txt")
  Io.println(s)
}
```

Corpus:

```bash
aura run corpus/std_io/app
aura run corpus/std_io/prelude
aura run corpus/std_io/files
# monorepo: cargo run -p aura-cli -- run corpus/std_io/files
```

## `std.assert`

Use with `aura test` and `@test` functions:

```bash
aura run corpus/std_assert/app
```

Prefer package tests that exercise `assert` / `assert_eq` for `Int` / `String` / `Bool` in the current MVP.

## `std.collections`

| Type / helper                            | Notes                                                |
| ---------------------------------------- | ---------------------------------------------------- |
| `Map<K, V>`                              | Linear map; `get` → `V?`; `put` / `remove` / `clear` |
| `Set<T>`                                 | Generic set (linear)                                 |
| `HashMap`                                | String→Int open addressing + auto-resize (C9b)       |
| `Iterable<E>`                            | `len` + `get` protocol for `for-in`                  |
| `map_ints` / `filter_ints` / `fold_ints` | Int array HOF helpers                                |

**Alpha limits:** no generic `HashMap<K,V>` yet (String→Int only for the hashed map). See [Arrays](./arrays.md) for HOF usage and capture limits.

```bash
aura run corpus/std_collections/app
aura run corpus/std_collections/hof
```

## How the CLI finds `std.*`

- Auto-prelude **`std.io`** for package builds
- Path resolution for `std.*` imports via `AURA_STD` or walk-up from the package

If imports fail, verify you are invoking the CLI on a **package directory** (with `aura.toml`) and that `std/` is reachable (monorepo layout, or your install’s std path when applicable).

## What is _not_ in core (by design)

Application frameworks, DI containers, ORM/HTTP stacks stay **out of core** RFCs. Expect those as ecosystem packages later, not as stdlib defaults.

## Next

- [Packages](./packages.md)
- [Testing](./testing.md)
- [RFC-007](/rfc/007)
