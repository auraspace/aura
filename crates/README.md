# Aura toolchain crates (Rust)

Implementation of the Aura compiler and CLI. User programs are written in **Aura**; this tree is the **Rust** host toolchain.

| Crate | Role |
| ----- | ---- |
| `aura-ast` | AST types |
| `aura-diagnostics` | line:col + pretty error snippets |
| `aura-lexer` | Tokenizer |
| `aura-parser` | Recursive-descent + Pratt parser |
| `aura-sema` | Name resolution + typecheck (classes, interfaces, generics+bounds) |
| `aura-codegen` | C backend (mono generics, interface tagged unions) |
| `aura-cli` | `aura` binary (`check` / `build` / `run` / `test` / `emit-c`; multi-file + `aura.toml`) |

Runtime stub: [`runtime/aura_rt.c`](../runtime/aura_rt.c).

See [docs/roadmap.md](../docs/roadmap.md) and RFC-001 §6.0 / RFC-004.

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/hello/main.aura
```
