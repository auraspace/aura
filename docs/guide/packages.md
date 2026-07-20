---
title: Packages
section: Toolchain
order: 50
summary: aura.toml, multi-file packages, imports, and path dependencies.
---

# Packages

Aura packages are the unit of multi-file compilation ([RFC-005](/rfc/005), [RFC-008](/rfc/008)).

## Layout

```text
my_app/
  aura.toml
  aura.lock          # written/verified for path deps
  src/
    main.aura
    util.aura
```

Minimal `aura.toml`:

```toml
[package]
name = "my_app"
version = "0.1.0"
```

## Multi-file same package

Files in the same package share the package namespace. Point the CLI at the **directory**:

```bash
cargo run -p aura-cli -- check corpus/multi
cargo run -p aura-cli -- run corpus/multi
cargo run -p aura-cli -- test corpus/multi
```

## Imports and visibility

- `import path.to.pkg` and `import path.to.pkg as Alias`
- `pub` controls cross-package visibility
- Path dependencies live under `[dependencies]` in `aura.toml`

Example shape (see `corpus/import/` for working samples):

```toml
[dependencies]
math = { path = "../math" }
```

```aura
import math

fun main() {
  // call into the math package
}
```

Qualified form:

```aura
import math as M
// M.someFun(...)
```

## Standard library

In-tree std packages include at least:

| Package      | Role                      |
| ------------ | ------------------------- |
| `std.io`     | `println` and I/O helpers |
| `std.assert` | assert helpers for tests  |

The CLI can auto-prelude `std.io` for package builds and resolve `std.*` path deps (see root README milestones C4g / C4h).

## Lockfile

`aura.lock` records path dependencies (including transitive) so builds stay reproducible within the monorepo workflow.

## Next

- [Testing](./testing.md)
- [CLI](./cli.md)
- [RFC-005](/rfc/005) — package manager design
