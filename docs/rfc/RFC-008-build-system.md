# RFC-008: Build System

| Field        | Value            |
| ------------ | ---------------- |
| **RFC**      | 008              |
| **Title**    | Build System     |
| **Status**   | Accepted         |
| **Layer**    | Toolchain        |
| **Authors**  |                  |
| **Created**  | 2026-07-15       |
| **Updated**  | 2026-07-16       |
| **Estimate** | 20–40 pages      |
| **Depends**  | RFC-004, RFC-005 |
| **Blocks**   | RFC-012, RFC-013 |

---

## 1. Abstract

This RFC defines how Aura **builds** packages into artifacts: target graph (bin/lib/test), feature flags, profiles (`dev`/`release`), caching, cross-compilation, and the default product—a **single native executable** linking user code and the runtime.

The build orchestrator is implemented in **Rust** and invoked via `aura build` / `aura check` / `aura test`.

## 2. Motivation

### 2.1 Problem statement

Compilation alone is not enough: users need reproducible, incremental builds, profiles, and cross targets without writing bespoke scripts.

### 2.2 Why now

Packages (RFC-005) and compiler (RFC-004) need a graph executor; distribution (RFC-013) consumes outputs.

### 2.3 Success metrics

| Metric           | Target                                     |
| ---------------- | ------------------------------------------ |
| Default artifact | One executable path                        |
| Incremental      | No-op rebuild ~instant when unchanged      |
| Cross            | Cross-compile to supported triples from CI |

## 3. Goals

- Clear target model and dependency graph.
- `dev` vs `release` profiles with sensible defaults.
- Cache compiled artifacts keyed by inputs.
- Cross-compile support for v1 platform matrix.
- Integration with tests and packaging.

## 4. Non-goals

- Generic multi-language monorepo build (Bazel replacement).
- Distributed remote execution in MVP.
- Dynamic plugin loading as default artifact type.

## 5. Prior art & alternatives

| System   | Notes                       | Take                    |
| -------- | --------------------------- | ----------------------- |
| Cargo    | Profiles, targets, features | **Primary inspiration** |
| Go build | Simple                      | UX contrast             |
| Gradle   | Flexible, heavy             | Avoid                   |
| Make     | Universal                   | Not primary UX          |

## 6. Design

### 6.1 Overview

```text
aura.toml targets
    → resolve packages (RFC-005)
    → build plan (topo units)
    → compile CGUs (RFC-004)
    → link runtime + objs
    → artifact in target/<profile>/
```

### 6.2 Targets

| Target kind | Output                                          |
| ----------- | ----------------------------------------------- |
| `bin`       | Executable (default ship form)                  |
| `lib`       | Library for dependents (rlib-like intermediate) |
| `test`      | Test harness binary                             |
| `bench`     | Later                                           |
| `example`   | Optional                                        |

Default binary name from package name.

### 6.3 Profiles

| Profile   | Goals                                                       |
| --------- | ----------------------------------------------------------- |
| `dev`     | Fast compile, debuginfo, fewer opts, race detector optional |
| `release` | LTO optional, strip, opts, no race detector                 |
| `test`    | Like dev + test cfg                                         |

User custom profiles in `aura.toml` (later).

### 6.4 Features

- Additive feature flags on packages (Cargo-like).
- `default` features set.
- Unify across graph.

### 6.5 Caching

- `target/` directory layout versioned.
- Key: compiler version, target triple, profile, features, source hash, dep hash.
- `aura clean` removes artifacts.

### 6.6 Cross-compilation

- `--target <triple>` for supported triples.
- Toolchain ships or downloads sysroot/runtime libs for target (RFC-013).
- Linker selection documented per host.

### 6.7 Linking model

- **Static link runtime by default** → single file.
- Optional dynamic (non-default) for advanced.
- LTO: **opt-in thin LTO** for release when stable (not mandatory day-one).

### 6.8 Build scripts

- **No arbitrary build scripts in MVP**; declarative settings in `aura.toml` only.
- If introduced later: sandboxed, no network default.

### 6.9 Examples

```text
aura build
aura build --release -o dist/server
aura build --target aarch64-unknown-linux-gnu
aura check
```

### 6.10 Error model / edge cases

| Case          | Behavior                              |
| ------------- | ------------------------------------- |
| Link error    | Surface linker message + Aura context |
| Cache corrupt | Rebuild unit; integrity hash fail     |
| Feature cycle | Resolve error                         |

### 6.11 Compatibility & migration

- `target/` format may bump with major toolchain.
- Stable: CLI flags for profile/target/output.

## 7. Open questions

| #   | Question                     | Options      | Owner | Status                                                                    |
| --- | ---------------------------- | ------------ | ----- | ------------------------------------------------------------------------- |
| 1   | Thin LTO default on release? | opt-in first | Build | **Resolved** — opt-in                                                     |
| 2   | build scripts MVP?           | no           | Build | **Resolved**                                                              |
| 3   | Intermediate lib format      |              | Build | **Resolved** — no stable lib format MVP; source→obj→link; rlib-like later |

## 8. Rationale & trade-offs

Cargo-like graphs match the package model and developer expectations for compiled languages. Single-file default matches RFC-000. Avoiding build scripts early reduces supply-chain risk. Cost: less flexibility for exotic codegen until later.

## 9. Unresolved / future work

- Remote cache
- Workspaces build scheduling UX
- Compile pipeline metrics HTML report

## 10. Security & safety considerations

- No ambient network during build in MVP.
- Cache poisoning mitigated by content hashes.
- Untrusted build scripts (if ever) sandboxed.

## 11. Implementation plan (optional)

| Phase | Scope                   | Exit criteria       |
| ----- | ----------------------- | ------------------- |
| B0    | Single bin local target | `aura build` hello  |
| B1    | Dep graph + cache       | Rebuild incremental |
| B2    | Release + cross         | Ship matrix CI      |

## 12. References

- Cargo reference: profiles, build cache
- RFC-004, RFC-005, RFC-006, RFC-013

---

## Changelog

| Date       | Author | Change                                                                                          |
| ---------- | ------ | ----------------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock no stable intermediate lib MVP; Status → **Accepted**                                      |
| 2026-07-16 |        | Status → **In Review** — Review: profiles/no-scripts direction solid; thin vs full build system |
| 2026-07-15 |        | Initial skeleton                                                                                |
| 2026-07-15 |        | Solid draft: profiles, single binary link                                                       |
| 2026-07-15 |        | Lock no build scripts MVP; LTO opt-in                                                           |
