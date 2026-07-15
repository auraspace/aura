# Aura corpus

Small `.aura` programs used as syntax fixtures for compiler milestone **C0** (`aura check`).

| Path | Intent |
| ---- | ------ |
| `hello/main.aura` | Package + `fun main` + call + string |
| `control/if_while.aura` | Params, types, `if`/`while`, locals |
| `types/nullable.aura` | `T?`, flow `!= null` / `== null`, `!!` |
| `expr/arith.aura` | Arithmetic, comparisons, `&&` |
| `expr/unary.aura` | `!` and negation |
| `fun/multi.aura` | Multiple top-level functions |
| `fun/nested_calls.aura` | Nested calls |
| `pkg/dotted.aura` | Dotted package path |
| `edge/empty_main.aura` | Empty function body |
| `edge/comments.aura` | Line and block comments |
| `class/greeter.aura` | Class, constructor, method, `this.field` |
| `class/counter.aura` | Mutable field + multi-class file |
| `iface/named.aura` | Interface + implements + upcast call |
| `generic/box.aura` | `class Box<T>` + monomorph ctor/method |
| `generic/id.aura` | `fun id<T>` monomorph |
| `generic/infer.aura` | Infer `Box("…")` / `id("…")` without `<T>` |
| `diag/undefined.aura` | **Expected fail** — diagnostics smoke (excluded from CI corpus) |
| `struct/point.aura` | Value `struct` fields + methods |
| `enum/color.aura` / `enum/result.aura` | Enums, match, `Result` |
| `control/try_catch.aura` | `throw` / `try` / `catch` (String/Int) |
| `control/class_throw.aura` | throw/catch class instances (C3g) |
| `control/for_range.aura` | `for (i in a..b)` exclusive range (C3h) |
| `control/break_continue.aura` | `break` / `continue` (C3i) |
| `control/for_in.aura` | `for (x in array)` element iteration (C3k) |
| `control/for_inclusive.aura` | `for (i in a..=b)` inclusive range (C3l) |
| `generic/array.aura` | Builtin `Array<T>` len/get/set (C3j) |
| `generic/array_push.aura` | `Array.push` + grow (C3m) |
| `generic/array_pop.aura` | `Array.pop` (C3r) |
| `test/smoke.aura` | `@test` + `assert` / `assert_eq` |
| `multi/` | Multi-file package + `aura.toml` (C3e) |
| `import/app` + `import/math` | `import` + `pub` + path dep (C3f); alias `Math.square` (C3n); `Math.Point` (C3u); `aura.lock` (C3p) |
| `import/collide` + lib_a/lib_b | same `fun add` in two packages (C3o); lockfile (C3p) |

All files must stay within [RFC-001 §6.0](../docs/rfc/RFC-001-language-specification.md) unless marked `// @requires: post-c1`.

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- check corpus/multi
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/multi
```
