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
  aura.lock          # generated/verified dependency pins
  src/
    main.aura
    util.aura
```

Minimal `aura.toml`:

```toml
[package]
name = "my_app"
version = "0.1.0"

[[bin]]
name = "my_app"
path = "src"
```

`version` here is the **app package** version (not the Aura toolchain version).
`[[bin]]` is optional; when omitted, Aura uses `src/` when present and derives
the binary name from the package. The scaffold commands generate both fields.

## Build profiles

The manifest accepts the built-in `dev`, `test`, and `release` profiles. Each
profile can inherit another built-in profile and override the C backend
settings:

```toml
[profile.release]
inherits = "dev"
optimization = "o2" # aliases: opt-level = "2"
debug = false       # alias: debug-info
lto = "off"         # off, thin, or full
detector = false    # alias: race-detector
panic = "abort"     # unwind or abort
backend = "c"
linker = "clang"
```

The parser normalizes all three profiles and rejects unknown keys, invalid
values, and inheritance cycles. The current CLI always builds through the C
backend; profile selection remains part of the backend/toolchain contract and
is not yet exposed as a command-line switch.

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

| Package                   | Role                                                                     |
| ------------------------- | ------------------------------------------------------------------------ |
| `std.io`                  | Console, file I/O, argv/stdin/exit, owned handles, and async descriptors |
| `std.assert`              | Runtime assertion primitive                                              |
| `std.test`                | Deterministic test assertions                                            |
| `std.collections`         | Map / Set / HashMap / HashSet / Iterable, snapshots, live views, HOFs    |
| `std.error`               | Shared errors and generic outcomes                                       |
| `std.bytes`               | Owned byte strings and buffers                                           |
| `std.encoding`            | UTF-8, hex, base64, and percent encoding                                 |
| `std.json`                | Bounded JSON validation and root values                                  |
| `std.mime`                | Media-type and filename sanitization                                     |
| `std.fs` / `std.os`       | Paths, filesystem metadata, environment, and process helpers             |
| `std.net` / `std.dns`     | Loopback TCP and numeric host resolution                                 |
| `std.url` / `std.http`    | URL helpers and bounded HTTP/1.1 client/server                           |
| `std.stream`              | Async reader/writer adapters                                             |
| `std.time` / `std.task`   | Monotonic timers and task lifecycle                                      |
| `std.sync` / `std.signal` | Synchronization and graceful shutdown state                              |
| `std.log` / `std.metrics` | Structured logging and counters                                          |

The CLI can auto-prelude `std.io` for package builds and resolve `std.*` path deps (via `AURA_STD` or walk-up). Details: [Standard library](./standard-library.md).

## Lockfile (alpha)

`aura.lock` records direct and transitive path dependencies (transitive entries
are marked with `# transitive`) so builds stay reproducible in the
monorepo/path-dependency workflow. If a lockfile exists, declared paths and
registry requirements must match it; mismatches fail the load instead of being
silently rewritten.

**Registry schema v0** may appear as structured entries (`version` / `source` / `checksum` form). Locked registry consumption now supports HTTPS metadata/archive fetch, semver pinning, SHA-256 verification, cache extraction, and offline locked inputs. A credentialed upload path exists in the CLI, but a stable public registry publication service, `git=`/`github=` sources, and workspaces remain deferred.

The current registry backend uses HTTPS metadata and archive downloads with
semver pinning, checksum verification, and cache extraction. The planned GitHub
index/Release `.crate` backend and direct `github = "owner/repo"` dependencies
remain design context in [RFC-005](../rfc/RFC-005-package-manager.md) §6.5–6.6.

## Publish and registry limits

- `aura publish --dry-run` is available for local validation and preview
- Registry upload is implemented behind an explicit registry URL and token;
  it is still a bounded alpha workflow rather than a public package service
- No `git=` / `github=` sources or workspaces
- Prefer monorepo-local or sibling `path = "…"` deps

See the [current 0.1.1-alpha release notes](../releases/0.1.1-alpha.md); the [0.1.0-alpha freeze](../releases/0.1.0-alpha.md) is historical.

## Next

- [Testing](./testing.md)
- [CLI](./cli.md)
- [RFC-005](/rfc/005) — package manager design
