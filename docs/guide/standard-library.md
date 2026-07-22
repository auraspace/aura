---
title: Standard library
section: Toolchain
order: 55
summary: In-tree std packages — std.io, std.assert, std.collections, and prelude resolution.
---

# Standard library

Aura’s **core** stdlib is intentionally small ([RFC-007](/rfc/007), [RFC-000](/rfc/000) batteries-included-but-modular). In this repository, packages live under `std/`.

## Packages today (post-alpha C12)

| Package           | Path              | Role                                                                      |
| ----------------- | ----------------- | ------------------------------------------------------------------------- |
| `std.io`          | `std/io`          | Console, file I/O, argv, stdin, exit                                      |
| `std.assert`      | `std/assert`      | Assert helpers for tests                                                  |
| `std.collections` | `std/collections` | Map/Set, generic hash collections and snapshots, `Iterable`, HOFs, `join` |

Builtins such as `Array<T>` and core scalars are part of the **language**, not a separate import. String methods (`indexOf`, `split`, `trim`, `toInt`, …) are language surface — see [Types](./types-and-nullability.md) and the [cheatsheet](./syntax-cheatsheet.md).

## `std.io`

Console, process, and file helpers (runtime `aura_*` intrinsics). Strict file APIs throw a `String` message on failure (missing path, I/O error, oversized file, embedded NUL). Soft `tryReadFile` returns `null` instead. Text is treated as a regular-file UTF-8 byte sequence (no embedded NUL); max size **256 MiB**.

### Console

| API                   | Role                               |
| --------------------- | ---------------------------------- |
| `print` / `println`   | stdout (no newline / with newline) |
| `eprint` / `eprintln` | stderr                             |

### Process (C12b–e)

| API                      | Role                                                                              |
| ------------------------ | --------------------------------------------------------------------------------- |
| `args(): Array<String>`  | Process argv; `[0]` = program name; user flags from index 1 (C12b)                |
| `readLine(): String?`    | One line without trailing `\n` / `\r\n`; `null` on EOF; empty line is `""` (C12d) |
| `readAllStdin(): String` | Remainder of stdin (throws on oversize / I/O / embedded NUL)                      |
| `exit(code: Int)`        | Terminate with status; flushes stdout/stderr first; does not return (C12e)        |

Pass user args after `--` with the CLI ([CLI](./cli.md)):

```bash
aura run my_pkg -- --flag value
cargo run -p aura-cli -- run corpus/std_io/args -- hello
printf 'line\n' | cargo run -p aura-cli -- run corpus/std_io/stdin
```

### Files (C11a / C12p)

| API                          | Role                                       |
| ---------------------------- | ------------------------------------------ |
| `readFile(path): String`     | read entire regular file (throws on error) |
| `tryReadFile(path): String?` | soft read; `null` on missing/error (C12p)  |
| `writeFile(path, content)`   | create/truncate and write                  |
| `appendFile(path, content)`  | append (create if needed)                  |
| `fileExists(path): Bool`     | regular file present                       |
| `fileSize(path): Int`        | byte size (throws if missing)              |

Typical use (explicit import or auto-prelude on package builds):

```aura
package main

import std.io as Io

fun main() {
  Io.println("Hello, Aura")
  val argv = Io.args()
  if (argv.len > 1) {
    Io.println(argv.get(1))
  }
  Io.writeFile("out.txt", "hi")
  val s = Io.tryReadFile("out.txt")
  if (s != null) {
    Io.println(s)
  }
}
```

Corpus:

```bash
aura run corpus/std_io/app
aura run corpus/std_io/prelude
aura run corpus/std_io/files
aura run corpus/std_io/try_read_file
aura run corpus/std_io/args -- hello
aura run corpus/std_io/stdin
aura run corpus/std_io/exit
# monorepo: cargo run -p aura-cli -- run corpus/std_io/files
```

Dogfood CLI that ties args + soft read + String tools: `examples/wc` ([README](https://github.com/auraspace/aura/blob/main/examples/wc/README.md)).

## `std.assert`

Use with `aura test` and `@test` functions:

```bash
aura run corpus/std_assert/app
```

Prefer package tests that exercise `assert` / `assert_eq` for `Int` / `String` / `Bool` in the current MVP.

## `std.collections`

| Type / helper                                     | Notes                                                                                |
| ------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `Map<K, V>`                                       | Linear map; `get` → `V?`; `put` / `remove` / `clear`                                 |
| `Set<T>`                                          | Generic set (linear)                                                                 |
| `HashMap<K,V>`                                    | Generic open addressing with `K: Hashable`; `containsValue` (C19a)                   |
| `HashSet<T>`                                      | Generic open addressing backed by `HashMap<T,Bool>`; `containsAll(Array<T>)` (C19a)  |
| `Hashable`                                        | `hash(): Int`; built-in for `Int` and `String` (C14)                                 |
| `keyArray()` / `valueArray()`                     | Live `HashMap` snapshots in logical table order (C18)                                |
| `HashMapEntry<K,V>` / `entries()`                 | Key/value snapshot pairs in logical table order (C19b)                               |
| `toArray()`                                       | Live `HashSet` snapshot in logical table order (C18)                                 |
| `map_hash_map_values`                             | Generic `(K,V) -> R` map-entry HOF (C18)                                             |
| `filter_hash_set` / `map_hash_set`                | Generic set HOFs returning arrays (C18)                                              |
| `Iterable<E>`                                     | `len` + `get` protocol for `for-in`, including `for (entry in map.entries())` (C19c) |
| `map<T,R>` / `filter<T>` / `fold<T,A>`            | Generic array HOFs; verified for `Int` and `String` (C16)                            |
| `map_ints` / `filter_ints` / `fold_ints`          | Int compatibility wrappers                                                           |
| `map_strings` / `filter_strings` / `fold_strings` | String compatibility wrappers (C12o)                                                 |
| `join(parts, sep)`                                | `Array<String>` → `String` with separator (C12j)                                     |

See [Arrays](./arrays.md) for HOF usage and capture limits.

```bash
aura run corpus/std_collections/app
aura run corpus/std_collections/hashmap
aura run corpus/std_collections/hashmap_str
aura run corpus/std_collections/hashmap_int
aura run corpus/std_collections/hashset_int
aura run corpus/std_collections/hof
aura run corpus/std_collections/hof_str
aura run corpus/std_collections/join
```

Hash collection HOFs are free functions because methods cannot declare their own
type parameters yet (C2b). They return arrays in logical table order and skip
empty/tombstone slots; they do not mutate the source collection.

`HashMap.entries()` likewise returns a fresh, shallow structural snapshot of
`HashMapEntry<K,V>` pairs. It preserves key/value pairing and can be consumed
directly with `for-in`, but it is not a live iterator or entry view: changing an
entry cannot mutate the source map.

## How the CLI finds `std.*`

- Auto-prelude **`std.io`** for package builds
- Path resolution for `std.*` (io / assert / collections):
  1. `AURA_STD` (directory that contains `io/`, `assert/`, …)
  2. Walk-up from the package looking for monorepo `std/<pkg>`
  3. Release install: `share/aura/std/<pkg>` next to the toolchain
  4. Embedded copy materialized under `~/.cache/aura/<version>/std/`

After a normal install (or `cargo install` of a recent CLI), you should **not** need to declare `std.io = { path = "..." }` in app `aura.toml`.

## What is _not_ in core (by design)

Application frameworks, DI containers, ORM/HTTP stacks stay **out of core** RFCs. Expect those as ecosystem packages later, not as stdlib defaults.

## Next

- [Packages](./packages.md)
- [CLI](./cli.md)
- [Testing](./testing.md)
- [RFC-007](/rfc/007)
