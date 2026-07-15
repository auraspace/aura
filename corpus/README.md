# Aura corpus

Small `.aura` programs used as syntax fixtures for compiler milestone **C0** (`aura check`).

| Path | Intent |
| ---- | ------ |
| `hello/main.aura` | Package + `fun main` + call + string |
| `control/if_while.aura` | Params, types, `if`/`while`, locals |
| `types/nullable.aura` | `T?` and `null` |
| `expr/arith.aura` | Arithmetic, comparisons, `&&` |
| `expr/unary.aura` | `!` and negation |
| `fun/multi.aura` | Multiple top-level functions |
| `fun/nested_calls.aura` | Nested calls |
| `pkg/dotted.aura` | Dotted package path |
| `edge/empty_main.aura` | Empty function body |
| `edge/comments.aura` | Line and block comments |

All files must stay within [RFC-001 §6.0](../docs/rfc/RFC-001-language-specification.md) unless marked `// @requires: post-c1`.

```bash
cargo run -p aura-cli -- check corpus/hello/main.aura
```
