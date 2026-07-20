# Aura corpus

Sample `.aura` programs for the compiler: parse/typecheck (`aura check`), native run (`aura run` / `aura build`), and `@test` (`aura test`). Layout tracks milestones through **C4t** (see [docs/roadmap.md](../docs/roadmap.md)).

## Core fixtures

| Path                    | Intent                                                                 |
| ----------------------- | ---------------------------------------------------------------------- |
| `hello/main.aura`       | Package + `fun main` + call + string                                   |
| `control/if_while.aura` | Params, types, `if`/`while`, locals                                    |
| `control/else_if.aura`  | `else if` chaining (C4l)                                               |
| `types/nullable.aura`   | `T?`, flow `!= null` / `== null`, `!!`                                 |
| `types/opt_prim.aura`   | `Int?` / `Bool?` tagged optional C emit (C7a)                          |
| `types/coalesce.aura`   | Null coalesce `?:` (C4m)                                               |
| `expr/arith.aura`       | Arithmetic, comparisons, `&&`                                          |
| `expr/unary.aura`       | `!` and negation                                                       |
| `expr/string_eq.aura`   | String content equality (C4e)                                          |
| `expr/string_len.aura`  | `String.len` byte length (C4p)                                         |
| `expr/if_expr.aura`     | `if` as expression (C4t)                                               |
| `fun/multi.aura`        | Multiple top-level functions                                           |
| `fun/nested_calls.aura` | Nested calls                                                           |
| `pkg/dotted.aura`       | Dotted package path                                                    |
| `edge/empty_main.aura`  | Empty function body                                                    |
| `edge/comments.aura`    | Line and block comments                                                |
| `diag/undefined.aura`   | **Expected fail** — diagnostics smoke (excluded from green run corpus) |

## Classes, interfaces, values

| Path                                   | Intent                                   |
| -------------------------------------- | ---------------------------------------- |
| `class/greeter.aura`                   | Class, constructor, method, `this.field` |
| `class/counter.aura`                   | Mutable field + multi-class file         |
| `class/identity.aura`                  | Class identity `==` / `!=` (C4a)         |
| `class/nullable.aura`                  | Nullable class `Class?` (C4b)            |
| `class/safe_call.aura`                 | Safe call `?.` (C4s)                     |
| `class/gc_array_field.aura`            | GC mark/free Array fields on class (C7b) |
| `class/alias_ref.aura`                 | Import alias type qualify / ctor (C3u)   |
| `iface/named.aura`                     | Interface + implements + upcast call     |
| `struct/point.aura`                    | Value `struct` fields + methods          |
| `enum/color.aura` / `enum/result.aura` | Enums, match, `Result`                   |

## Control flow & exceptions

| Path                          | Intent                                       |
| ----------------------------- | -------------------------------------------- |
| `control/try_catch.aura`      | `throw` / `try` / `catch` (String/Int)       |
| `control/class_throw.aura`    | throw/catch class instances (C3g)            |
| `control/for_range.aura`      | `for (i in a..b)` exclusive range (C3h)      |
| `control/for_inclusive.aura`  | `for (i in a..=b)` inclusive range (C3l)     |
| `control/break_continue.aura` | `break` / `continue` (C3i)                   |
| `control/for_in.aura`         | `for (x in array)` (C3k)                     |
| `control/for_in_string.aura`  | `for (b in string)` UTF-8 bytes as Int (C3w) |

## Generics & Array

| Path                             | Intent                                        |
| -------------------------------- | --------------------------------------------- |
| `generic/box.aura`               | `class Box<T>` monomorph ctor/method          |
| `generic/id.aura`                | `fun id<T>` monomorph                         |
| `generic/infer.aura`             | Infer `Box("…")` / `id("…")` without `<T>`    |
| `generic/bounds.aura`            | Type-param bounds + method recv (C2e/C4k)     |
| `generic/array.aura`             | Builtin `Array<T>` len/get/set (C3j)          |
| `generic/array_push.aura`        | `Array.push` + grow (C3m)                     |
| `generic/array_pop.aura`         | `Array.pop` (C3r)                             |
| `generic/array_clear.aura`       | `Array.clear` (C4f)                           |
| `generic/array_isempty.aura`     | `Array.isEmpty` (C4n)                         |
| `generic/array_reserve.aura`     | `Array.reserve` (C4o)                         |
| `generic/array_class.aura`       | `Array` of class refs (C4c)                   |
| `generic/array_struct.aura`      | `Array` of struct by-value (C4q)              |
| `generic/array_enum.aura`        | `Array` of enum by-value (C6g)                |
| `generic/array_enum_result.aura` | `Array` of generic enum `Result` (C6g)        |
| `generic/array_reassign.aura`    | Free Array buffer on owner reassignment (C4r) |

## Packages, import, stdlib

| Path                                  | Intent                                                                                              |
| ------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `test/smoke.aura`                     | `@test` + `assert` / `assert_eq`                                                                    |
| `multi/`                              | Multi-file package + `aura.toml` (C3e)                                                              |
| `import/app` + `import/math`          | `import` + `pub` + path dep (C3f); alias `Math.square` (C3n); `Math.Point` (C3u); `aura.lock` (C3p) |
| `import/collide` + lib_a/lib_b        | same `fun add` (C3o) + same `class Token` (C3v); lockfile (C3p)                                     |
| `import/iface_app` + iface_a/iface_b  | same interface name across packages (C4d)                                                           |
| `import/nested_app` + nested_mid/leaf | Nested path deps in `aura.lock` (C4j)                                                               |
| `std_io/app`                          | Explicit `import std.io` + `println` (C3z)                                                          |
| `std_io/prelude`                      | Auto-prelude `std.io` without import (C4g)                                                          |
| `std_assert/app`                      | `std.assert` package (C4h)                                                                          |

Std packages live under repo `std/io` and `std/assert` (path-resolved for `std.*`).

## Notes

- Prefer staying within the documented language surface; experimental files may note a milestone in the table above.
- Expected-fail diagnostics: `diag/undefined.aura` is not part of the green run set.

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/hello/main.aura
cargo run -p aura-cli -- check corpus/multi
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/multi
cargo run -p aura-cli -- run corpus/import/app
cargo run -p aura-cli -- run corpus/std_io/prelude
```
