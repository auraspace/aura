---
title: Syntax cheatsheet
section: Language
order: 39
summary: Compact lookup for keywords, types, and common forms.
---

# Syntax cheatsheet

Non-normative. Source of truth: [RFC-001](/rfc/001).

## File skeleton

```aura
package main

fun main() {
  println("hi")
}
```

## Declarations

| Form            | Example                                         |
| --------------- | ----------------------------------------------- |
| Function        | `fun add(a: Int, b: Int): Int { return a + b }` |
| Async function  | `async fun load(): String { return "ok" }`      |
| Expr-body fun   | `fun double(x: Int): Int = x * 2`               |
| Local           | `val x = 1` / `var y = 2`                       |
| Class           | `class C(var n: Int) { fun f() {} }`            |
| Struct          | `struct S(var x: Int) {}`                       |
| Interface       | `interface I { fun f(): Int }`                  |
| Implements      | `class C() : I { ... }` / `class Box<T> : I<T>` |
| Enum            | `enum E { case A, case B }`                     |
| Generic class   | `class Box<T>(var v: T) {}`                     |
| Generic fun     | `fun id<T>(x: T): T { return x }`               |
| Type alias      | `type Id = Int`                                 |
| Top-level const | `const N: Int = 42`                             |
| Test            | `@test fun t() { assert_eq(1, 1) }`             |
| Attribute       | `@deprecated("use newName")`                    |

## Types

| Form                        | Meaning                            |
| --------------------------- | ---------------------------------- |
| `Int` `Bool` `String`       | Scalars                            |
| `T?`                        | Nullable                           |
| `Array<T>`                  | Array                              |
| `Result<T, E>`              | Success / error                    |
| `(T) -> U`                  | Function type (params → result)    |
| `T : Bound`                 | Type param bound                   |
| `ref T`                     | Scoped non-owning reference        |
| `Task<T>` / `TaskHandle<T>` | Async result / spawned task handle |
| `Channel<T>`                | Bounded async FIFO channel         |

## Lambdas (C10 + C12 captures)

```aura
val f = (x: Int) => x + 1
val g: (Int) -> Int = (x: Int) => x * 2
val h = (x: Int) => {
  val y = x + 1
  return y * 2
}
// Captures: val scalar/String/class/Array/Fun; var scalar/String/class/Array/Fun
// through shared mutable boxes (C20c-e).
val base = 10
val add = (x: Int) => base + x
```

| Capture                                         | MVP rule                                                                                 |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `val` Int / Bool / String                       | Copy into env (C10h)                                                                     |
| `val` class                                     | GC ptr in env; env mark walks roots (C12k)                                               |
| `val` Array                                     | Owned snapshot when captured; field borrows remain lexical (C12l)                         |
| `var` Int / Bool / String / class / Array / Fun | Shared mutable box; lambdas share writes (C12m, C20c-e)                                  |
| Captured Array ownership / live view            | Immutable captures own snapshots; mutable captures use retained shared cells             |

## Async and tasks

```aura
async fun answer(): Int { return 42 }

fun main() {
  val task: TaskHandle<Int> = spawn {
    return await answer()
  }
  join(task)
  cancel(task)
}
```

`await` is valid inside `async fun` and spawned bodies. `Channel<T>(capacity)`
provides bounded `send`, `receive`, and `close` operations. See
[Async, tasks & borrowing](./async-and-borrowing.md) for ownership boundaries.

## Attributes

Common supported forms:

```aura
@test(tag = "fast")
fun testFast() { assert_eq(1, 1) }

@derive(Equals, HashCode, Debug)
struct Point(val x: Int, val y: Int) {}

@deprecated(since = "0.1.1")
fun oldApi() {}
```

See [Attributes & derives](./attributes-and-derives.md) for targets and
validation rules.

## Operators (common)

| Group      | Forms             |
| ---------- | ----------------- |
| Arithmetic | `+ - * / %`       |
| Compare    | `== != < <= > >=` |
| Logic      | `&& \|\| !`       |
| Null       | `?:` `!!`         |
| Range      | `a..b` `a..=b`    |

Class `==` is **identity**. String content equality uses content compare in the current path; struct/enum equality is restricted in sema.

## String helpers (MVP)

| Form                                                  | Notes                                                                                     |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `s + t` / `"hi ${name}"`                              | Concat; interp desugars to `+` (idents in `${…}`)                                         |
| `s.len` / `s.isEmpty()`                               | UTF-8 **byte** length                                                                     |
| `s.charAt(i)`                                         | Byte as `Int`; OOB throws                                                                 |
| `s.startsWith(x)` / `s.contains(x)` / `s.endsWith(x)` | Substring search                                                                          |
| `s.indexOf(sub)`                                      | Byte index of first match; −1 if missing; empty sub → 0 (C12f)                            |
| `s.split(sep)`                                        | `Array<String>`; empty sep throws; consecutive/trailing seps → empty segments (C12g)      |
| `s.trim()` / `trimStart` / `trimEnd`                  | ASCII whitespace MVP (`' '`, `\t`, `\n`, `\r`); owned copy (C12h)                         |
| `s.toInt()`                                           | `Int?`; full-string decimal; no auto-trim; optional `+/-`; invalid/overflow → null (C12i) |
| `join(parts, sep)`                                    | `std.collections`: `Array<String>` + sep → `String`; empty → `""` (C12j)                  |
| `s.substring(start, end)`                             | Exclusive end; UTF-8 **byte** indices (C11d)                                              |

No embedded NUL in strings. Indices are bytes, not Unicode scalar values.

## Process I/O (`std.io`, C12b–e / C12p)

| Form                     | Notes                                                              |
| ------------------------ | ------------------------------------------------------------------ |
| `args(): Array<String>`  | Process argv; `[0]` = program name; user flags from index 1 (C12b) |
| `readLine(): String?`    | One line without trailing newline; `null` on EOF (C12d)            |
| `readAllStdin(): String` | Remainder of stdin (throws on oversize / error)                    |
| `exit(code: Int)`        | Terminate with status; flushes stdio (C12e)                        |
| `tryReadFile(path)`      | `String?` soft file read; `null` on missing/error (C12p)           |

Pass process args after `--`:

```bash
aura run path -- flag value
aura test path -- …
```

## Control

```aura
if (cond) { } else if (other) { } else { }

while (cond) { break; continue }

for (i in 0..n) { }
for (i in 0..=n) { }
for (x in xs) { }

match (e) {
  case Pattern => { }
}

try { } catch (e: String) { } finally { }
throw "msg"

// scoped borrow
fun size(xs: Array<Int>): Int {
  val view: ref Array<Int> = xs
  return view.len
}
```

## Packages & imports

```aura
package app

import math
import math as M
```

```toml
# aura.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
math = { path = "../math" }
```

Root procedural derive plugin:

```toml
[macro_plugins]
Entity = "plugins/entity-macro"
```

## CLI one-liners

```bash
# After install (or with aura on PATH):
aura new hello && aura run hello
aura check path
aura run path
aura run path -- a b
aura build path -o out
aura test path
aura version

# In-tree monorepo:
cargo run -p aura-cli -- run path
cargo run -p aura-cli -- run examples/wc -- file.txt
```

## Next

- [Language tour](./language-tour.md)
- [FAQ](./faq.md)
