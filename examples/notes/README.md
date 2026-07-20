# examples/notes

Dogfood package: a small **line-oriented notes** app that exercises packages, classes, `Array`, lambdas-adjacent HOF surface, `std.io` file I/O, `String.substring` / `charAt`, and `aura test`.

## Run

From the monorepo root (so `std/` and `runtime/` resolve):

```bash
cargo run -p aura-cli -- run examples/notes
cargo run -p aura-cli -- test examples/notes
cargo run -p aura-cli -- check examples/notes
```

Demo writes `target/aura/examples_notes.txt`.

## Layout

| Path                | Role                                    |
| ------------------- | --------------------------------------- |
| `src/notebook.aura` | `Notebook` class + `notebook()` factory |
| `src/main.aura`     | Demo `main` + `@test` suite             |

## Limits (honest)

- No CLI args yet — `main` runs a fixed scenario.
- Notes are one line each (newline is the separator).
- String indices are UTF-8 **bytes** (same as the rest of the String surface).
