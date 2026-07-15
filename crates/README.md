# Aura toolchain crates (Rust)

Implementation of the Aura compiler and CLI. User programs are written in **Aura**; this tree is the **Rust** host toolchain.

| Crate | Role (C0) |
| ----- | --------- |
| `aura-ast` | AST types |
| `aura-lexer` | Tokenizer |
| `aura-parser` | Recursive-descent + Pratt parser |
| `aura-cli` | `aura` binary (`check`) |

See [docs/roadmap.md](../docs/roadmap.md) and RFC-001 §6.0 / RFC-004.

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura
```
