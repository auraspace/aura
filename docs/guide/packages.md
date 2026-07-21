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

`version` here is the **app package** version (not the Aura toolchain version).

## Multi-file same package

Files in the same package share the package namespace. Point the CLI at the **directory**:

```bash
aura check corpus/multi
aura run corpus/multi
aura test corpus/multi

# monorepo without global install:
cargo run -p aura-cli -- run corpus/multi
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
// M.SomeType(...)
```

## Standard library

In-tree std packages (alpha):

| Package           | Role                                                                  |
| ----------------- | --------------------------------------------------------------------- |
| `std.io`          | Console, file I/O, argv/stdin/exit (`println`, `args`, `readFile`, …) |
| `std.assert`      | Assert helpers for tests                                              |
| `std.collections` | Map / Set / HashMap / HashSet / Iterable + Int·String HOF + `join`    |

The CLI can auto-prelude `std.io` for package builds and resolve `std.*` path deps (via `AURA_STD` or walk-up). Details: [Standard library](./standard-library.md).

## Lockfile (alpha)

`aura.lock` records path dependencies (including transitive `# transitive` lines) so builds stay reproducible in the monorepo / path-dep workflow.

**Registry schema v0** may appear as structured entries (`version` / `source` / `checksum` form) for forward compatibility — **there is no registry fetch or semver resolve yet**. Multi-package work in alpha is **path deps only**.

When registry lands, the **default backend is GitHub** (index repo + Release `.crate` assets, plus direct `github = "owner/repo"` deps). Design: [RFC-005](../rfc/RFC-005-package-manager.md) §6.5–6.6.

## Alpha limits

- No package registry download / publish
- No caret/semver range resolution
- Prefer monorepo-local or sibling `path = "…"` deps

See [0.1.0-alpha freeze](https://github.com/auraspace/aura/blob/main/docs/releases/0.1.0-alpha.md).

## Next

- [Testing](./testing.md)
- [CLI](./cli.md)
- [RFC-005](/rfc/005) — package manager design
