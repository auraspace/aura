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

## Root procedural macro plugins

A package may opt into a versioned, sandboxed executable for a derive by adding
`[macro_plugins]`. Paths are relative to the package root and only the root
package's declarations are executed:

```toml
[macro_plugins]
Entity = "plugins/entity-macro"
```

When a class uses `@derive(Entity)`, `aura check`, `aura emit-c`, `aura build`,
and `aura test` run that executable through the RFC-010 protocol. Generated
source is parsed, package identity is checked, merged, and semantically checked
before code generation. Plugin output and runtime are bounded by the sandbox's
configured timeout and output limit; dependency-provided plugins are not
implicitly executed.

Root plugin executables are pinned in `aura.lock` as
`macro_plugin.<Name>` entries with their package-relative path and SHA-256
checksum. Read-only tooling requires an existing matching pin, and a changed
executable is rejected until the lock is intentionally refreshed.

## Multi-file same package

Files in the same package share the package namespace. Point the CLI at the **directory**:

```bash
aura check corpus/multi
aura run corpus/multi
aura test corpus/multi

# monorepo without global install:
cargo run -p aura-cli -- run corpus/multi
```

## Dependency commands

Use the flat CLI commands to edit dependencies and refresh the lockfile:

```bash
aura add auraspace/aura@v0.1.1-alpha.6 --subdir std/io
aura remove demo.dep
```

`aura add` is transactional: if resolution or integrity verification fails,
both `aura.toml` and `aura.lock` are restored.

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
| `std.net` / `std.dns`     | Endpoint-aware TCP and numeric host resolution                           |
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

Locked origin consumption supports Git tag resolution, source/archive fetch,
semver pinning, immutable commit pins, SHA-256 verification, cache extraction,
and offline locked inputs. Direct dependencies can use
`{ git = "https://…", tag = "vX.Y.Z" }`, `{ git = "https://…", rev = "…" }`,
or `{ github = "owner/repo", tag = "vX.Y.Z" }`. The public design follows Go:
a public Git repository plus an immutable `vX.Y.Z` tag is enough to publish. A
proxy and checksum database are later layers; the proxy read shapes are reserved
without changing origin identity.

The current client resolves Git tags/revisions directly, archives the selected
source tree, pins the commit and checksum, and extracts it into the cache. A
future proxy may expose the same Go-shaped read objects (`@v/list`, `.info`,
`.mod`, and `.zip`) without changing the origin identity. See
[RFC-005](../rfc/RFC-005-package-manager.md) §6.6.

## Publish and registry limits

- Package publication uses an immutable `vX.Y.Z` Git tag pushed to the origin
- The former HTTP upload path has been removed; publication is origin-based
- Public publication is a Git operation: create and push an immutable origin tag
- Git origins may select a normalized monorepo package with `subdir = "path/to/package"`
- GitHub Releases are optional for packages and reserved primarily for binaries
- A proxy/cache and checksum database are deliberately deferred
- Workspaces remain separate package-manager work; direct `git=` / `github=`
  sources are supported
- Prefer monorepo-local or sibling `path = "…"` deps

See the [current 0.1.1-alpha.6 release notes](../releases/0.1.1-alpha.6.md); the [0.1.0-alpha freeze](../releases/0.1.0-alpha.md) is historical.

## Next

- [Testing](./testing.md)
- [CLI](./cli.md)
- [RFC-005](/rfc/005) — package manager design
