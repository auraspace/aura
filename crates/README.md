# Aura toolchain crates (Rust)

Implementation of the Aura compiler and CLI. User programs are written in **Aura**; this tree is the **Rust** host toolchain.

| Crate              | Role                                                                                                       |
| ------------------ | ---------------------------------------------------------------------------------------------------------- |
| `aura-ast`         | AST types                                                                                                  |
| `aura-diagnostics` | line:col + pretty error snippets                                                                           |
| `aura-lexer`       | Tokenizer                                                                                                  |
| `aura-parser`      | Recursive-descent + Pratt parser                                                                           |
| `aura-sema`        | Name resolution + typecheck (classes, interfaces, generics+bounds, null flow, fun types/lambdas)           |
| `aura-codegen`     | C backend (mono generics, GC class refs, Array, exceptions, fat-pointer Fun)                               |
| `aura-cli`         | `aura` binary (`check` / `build` / `run` / `test` / `emit-c`; multi-file + `aura.toml` + path deps / lock) |

Runtime: [`runtime/aura_rt.c`](../runtime/aura_rt.c) (println, exceptions, Array helpers, GC mark/sweep).

Milestone status: [docs/roadmap.md](../docs/roadmap.md) (through **C10j**). Language MVP freeze: RFC-001 §6.0; architecture: RFC-004. Corpus: [`corpus/`](../corpus/).

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura
cargo run -p aura-cli -- run corpus/hello/main.aura
cargo run -p aura-cli -- test corpus/test/smoke.aura
```
