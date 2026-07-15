# Aura toolchain crates (Rust)

Implementation of the Aura compiler and CLI. User programs are written in **Aura**; this tree is the **Rust** host toolchain.

| Crate | Role |
| ----- | ---- |
| `aura-ast` | AST types |
| `aura-lexer` | Tokenizer |
| `aura-parser` | Recursive-descent + Pratt parser |
| `aura-sema` | Name resolution + typecheck (C0+) |
| `aura-codegen` | C backend codegen (C1) |
| `aura-cli` | `aura` binary (`check` / `build` / `run` / `emit-c`) |

Runtime stub: [`runtime/aura_rt.c`](../runtime/aura_rt.c).

See [docs/roadmap.md](../docs/roadmap.md) and RFC-001 §6.0 / RFC-004.

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/hello/main.aura
```
