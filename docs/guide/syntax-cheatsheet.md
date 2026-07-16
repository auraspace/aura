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

| Form | Example |
| ---- | ------- |
| Function | `fun add(a: Int, b: Int): Int { return a + b }` |
| Local | `val x = 1` / `var y = 2` |
| Class | `class C(var n: Int) { fun f() {} }` |
| Struct | `struct S(var x: Int) {}` |
| Interface | `interface I { fun f(): Int }` |
| Implements | `class C() implements I { ... }` |
| Enum | `enum E { A, B }` |
| Generic class | `class Box<T>(var v: T) {}` |
| Generic fun | `fun id<T>(x: T): T { return x }` |
| Test | `@test fun t() { assert_eq(1, 1) }` |

## Types

| Form | Meaning |
| ---- | ------- |
| `Int` `Bool` `String` | Scalars |
| `T?` | Nullable |
| `Array<T>` | Array |
| `Result<T, E>` | Success / error |
| `T : Bound` | Type param bound |

## Operators (common)

| Group | Forms |
| ----- | ----- |
| Arithmetic | `+ - * / %` |
| Compare | `== != < <= > >=` |
| Logic | `&& \|\| !` |
| Null | `?:` `!!` |
| Range | `a..b` `a..=b` |

Class `==` is **identity**. String content equality uses content compare in the current path; struct/enum equality is restricted in sema.

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
cargo run -p aura-cli -- check path
cargo run -p aura-cli -- run path
cargo run -p aura-cli -- build path -o out
cargo run -p aura-cli -- test path
```

## Next

- [Language tour](./language-tour.md)
- [FAQ](./faq.md)
