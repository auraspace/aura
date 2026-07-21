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
| Expr-body fun   | `fun double(x: Int): Int = x * 2`               |
| Local           | `val x = 1` / `var y = 2`                       |
| Class           | `class C(var n: Int) { fun f() {} }`            |
| Struct          | `struct S(var x: Int) {}`                       |
| Interface       | `interface I { fun f(): Int }`                  |
| Implements      | `class C() : I { ... }` / `class Box<T> : I<T>` |
| Enum            | `enum E { A, B }`                               |
| Generic class   | `class Box<T>(var v: T) {}`                     |
| Generic fun     | `fun id<T>(x: T): T { return x }`               |
| Type alias      | `type Id = Int`                                 |
| Top-level const | `const N: Int = 42`                             |
| Test            | `@test fun t() { assert_eq(1, 1) }`             |

## Types

| Form                  | Meaning                         |
| --------------------- | ------------------------------- |
| `Int` `Bool` `String` | Scalars                         |
| `T?`                  | Nullable                        |
| `Array<T>`            | Array                           |
| `Result<T, E>`        | Success / error                 |
| `(T) -> U`            | Function type (params → result) |
| `T : Bound`           | Type param bound                |

## Lambdas (C10)

```aura
val f = (x: Int) => x + 1
val g: (Int) -> Int = (x: Int) => x * 2
val h = (x: Int) => {
  val y = x + 1
  return y * 2
}
// Captures: outer immutable val of Int / Bool / String only (MVP).
val base = 10
val add = (x: Int) => base + x
```

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

| Form                                     | Notes                                                                                     |
| ---------------------------------------- | ----------------------------------------------------------------------------------------- |
| `s + t` / `"hi ${name}"`                 | Concat; interp desugars to `+` (idents in `${…}`)                                         |
| `s.len` / `s.isEmpty()`                  | UTF-8 **byte** length                                                                     |
| `s.charAt(i)`                            | Byte as `Int`; OOB throws                                                                 |
| `s.startsWith` / `contains` / `endsWith` | Substring search                                                                          |
| `s.indexOf(sub)`                         | Byte index of first match; −1 if missing; empty sub → 0 (C12f)                            |
| `s.split(sep)`                           | `Array<String>`; empty sep throws; consecutive/trailing seps → empty segments (C12g)      |
| `s.trim()` / `trimStart` / `trimEnd`     | ASCII whitespace MVP (`' '`, `\\t`, `\\n`, `\\r`); owned copy (C12h)                      |
| `s.toInt()`                              | `Int?`; full-string decimal; no auto-trim; optional `+/-`; invalid/overflow → null (C12i) |
| `join(parts, sep)`                       | `std.collections`: `Array<String>` + sep → `String`; empty → `""` (C12j)                  |
| `s.substring(start, end)`                | Exclusive end; UTF-8 **byte** indices (C11d)                                              |

No embedded NUL in strings. Indices are bytes, not Unicode scalar values.

## Control

```aura
if (cond) { } else if (other) { } else { }

while (cond) { break; continue }

for (i in 0..n) { }
for (i in 0..=n) { }
for (x in xs) { }

match (e) {
  Pattern => { }
}

try { } catch (e: String) { } finally { }
throw "msg"
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

## CLI one-liners

```bash
# After install (or with aura on PATH):
aura new hello && aura run hello
aura check path
aura run path
aura build path -o out
aura test path
aura version

# In-tree monorepo:
cargo run -p aura-cli -- run path
```

## Next

- [Language tour](./language-tour.md)
- [FAQ](./faq.md)

## Next

- [Language tour](./language-tour.md)
- [FAQ](./faq.md)

# In-tree monorepo:

cargo run -p aura-cli -- run path

```

## Next

- [Language tour](./language-tour.md)
- [FAQ](./faq.md)
```
