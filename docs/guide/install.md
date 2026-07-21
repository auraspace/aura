---
title: Install
section: Start
order: 15
summary: Install the aura CLI with versioned $AURA_HOME layout and switch later.
---

# Install

Aura’s toolchain is the **`aura` CLI** (Rust crate `aura-cli`). User programs compile through a **C backend** and need a system C compiler at **build** time.

## One-liner (release tarball)

`v0.1.0-alpha` assets are on [GitHub Releases](https://github.com/auraspace/aura/releases/tag/v0.1.0-alpha):

```bash
curl -fsSL https://aura.fadosoft.com/install.sh | bash
```

### Versioned layout (`$AURA_HOME`, default `~/.aura`)

```text
$AURA_HOME/
  versions/
    0.1.0-alpha/
      bin/aura
      share/aura/aura_rt.c    # from the release archive (optional)
      meta/version, os, arch, installed_at
    0.2.0/
      bin/aura
      …
  current -> versions/0.1.0-alpha     # active toolchain
  bin/
    aura -> ../current/bin/aura       # put this on PATH
    avm                               # Aura Version Manager
```

The installer also symlinks `~/.local/bin/aura` and `~/.local/bin/avm` (disable with `AURA_LINK_USER_BIN=0`).

### Options

```bash
# Pin a version (tag without leading v)
curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=0.1.0-alpha bash

# Custom home (multi-user or CI)
curl -fsSL https://aura.fadosoft.com/install.sh | AURA_HOME=/opt/aura bash

# Install side-by-side without changing the active version
curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=0.2.0 AURA_SET_DEFAULT=0 bash
avm 0.2.0
```

### Switch versions

```bash
avm --list
avm --show
avm 0.1.0-alpha
aura version
```

`avm` only flips the `current` symlink; previously installed trees under `versions/` stay on disk.

Source of truth: [`scripts/install.sh`](https://github.com/auraspace/aura/blob/main/scripts/install.sh) + [`scripts/avm`](https://github.com/auraspace/aura/blob/main/scripts/avm). Site build (`site/scripts/sync-install.mjs`) embeds `avm` into `public/install.sh` for the CDN.

## Prerequisites

| Tool                        | Why                              |
| --------------------------- | -------------------------------- |
| **Rust** (stable)           | Build / install the CLI (source) |
| **`cc`** (`clang` or `gcc`) | Link Aura → C → native binary    |
| **curl** + **tar**          | One-liner installer              |
| **Git**                     | Clone the repository (source)    |

## Install from source (alpha)

`cargo install` puts a **single** binary in `~/.cargo/bin` (not versioned under `$AURA_HOME`). Prefer the one-liner for normal use.

```bash
git clone https://github.com/auraspace/aura.git
cd aura
cargo install --path crates/aura-cli
```

```bash
aura version
aura new hello
cd hello && aura run .
```

### Runtime library

The C runtime (`runtime/aura_rt.c`) is **embedded** in the CLI and written to a cache on first build if no on-disk copy is found:

| Location                                | When used                     |
| --------------------------------------- | ----------------------------- |
| `AURA_RUNTIME`                          | Explicit override (file path) |
| Monorepo `runtime/aura_rt.c`            | Dev / `cargo run -p aura-cli` |
| `$AURA_HOME/versions/*/share/aura/`     | From release tarball          |
| Next to the binary                      | Optional layout               |
| `~/.cache/aura/<cli-version>/aura_rt.c` | Materialized from the embed   |

### Standard library (`std.io`, …)

Auto-prelude and `import std.*` resolve packages from:

| Location                                         | When used                         |
| ------------------------------------------------ | --------------------------------- |
| `AURA_STD`                                       | Explicit root containing `io/`, … |
| Monorepo `std/<pkg>` (walk-up from package root) | Dev workflow                      |
| `$AURA_HOME/.../share/aura/std/<pkg>`            | From release tarball              |
| Next to the binary `share/aura/std/<pkg>`        | Unpacked archive layout           |
| `~/.cache/aura/<cli-version>/std/`               | Materialized from the embed       |

You do **not** need to keep the git clone after install for compiling Aura programs.

## Without installing

From a clone:

```bash
cargo run -p aura-cli -- version
cargo run -p aura-cli -- run examples/notes
```

## Release archives

Pushing a tag `v*` runs [`.github/workflows/release.yml`](../../.github/workflows/release.yml): build tarballs (Linux amd64, macOS arm64/amd64) and attach them to a **GitHub Release**.

Maintainer flow (version dump → changelog → commit → tag → CI):

```text
prepare-release.sh → push commit → tag v* → Actions Release → install.sh
```

Full steps: [`docs/releases/README.md`](../releases/README.md).

```bash
# After the release is published:
tar xzf aura-*-<os>-<arch>.tar.gz
export PATH="$PWD/aura-*/bin:$PATH"
aura version
```

Or use the installer (recommended): it unpacks into `$AURA_HOME/versions/<ver>/`.

Local package without publishing:

```bash
TAG_VERSION=0.1.0-alpha bash scripts/package-release.sh
# → dist/aura-0.1.0-alpha-<os>-<arch>.tar.gz
```

## Verify install

```bash
export PATH="$HOME/.aura/bin:$HOME/.local/bin:$PATH"
aura version
avm --show
aura new /tmp/aura-smoke && aura run /tmp/aura-smoke
```

Expect `Hello, Aura` on stdout.

## Troubleshooting

| Symptom                  | Fix                                                                         |
| ------------------------ | --------------------------------------------------------------------------- |
| `cc` / `clang` not found | Install Xcode CLT (macOS) or `build-essential` (Debian/Ubuntu)              |
| `cannot find runtime`    | Upgrade CLI (embed) or set `AURA_RUNTIME` to a valid `aura_rt.c`            |
| `std.io` / std not found | Upgrade CLI (embed + `share/aura/std`) or set `AURA_STD` to monorepo `std/` |
| Wrong CLI                | `which aura` / `avm --show` — prefer `$AURA_HOME/bin`                       |
| Old binary on PATH       | Ensure `$AURA_HOME/bin` or `~/.local/bin` precedes `~/.cargo/bin`           |

## Next

- [Getting started](./getting-started.md)
- [CLI](./cli.md)
- [Release notes 0.1.0-alpha](../releases/0.1.0-alpha.md)
