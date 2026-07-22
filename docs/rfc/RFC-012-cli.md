# RFC-012: CLI

| Field        | Value                              |
| ------------ | ---------------------------------- |
| **RFC**      | 012                                |
| **Title**    | CLI                                |
| **Status**   | Accepted                           |
| **Layer**    | Toolchain                          |
| **Authors**  |                                    |
| **Created**  | 2026-07-15                         |
| **Updated**  | 2026-07-22                         |
| **Estimate** | 20–30 pages                        |
| **Depends**  | RFC-005, RFC-008, RFC-011, RFC-013 |
| **Blocks**   | —                                  |

---

## 1. Abstract

This RFC defines the unified **`aura` CLI** (implemented in **Rust**): the single entrypoint for create, build, run, test, check, format, package, and toolchain management. Subcommands delegate to compiler, package manager, build, and test subsystems while presenting a consistent UX, exit codes, and machine-readable output modes.

**Toolchain today (2026-07-22, S2/C21i):** shipped subcommands — `new`, `init`, `version`, `check`, `build`, `run`, `test`, `fmt`, and `emit-c` — on files or package dirs (`aura.toml`). `run`/`test` forward args after `--`; `test` supports name filters and structured JSON reports. Programs read argv via `std.io.args()`. Pretty diagnostics include path, line/column, source context, and notes; structured diagnostic models/JSON are available to tooling. Locked registry dependencies are supported by the package commands. Not yet: `add`, `publish`, release profiles beyond basic link, or a full `aura toolchain` manager.

## 2. Motivation

### 2.1 Problem statement

Fragmented tools (`fmt`, `build`, `pkg` as separate binaries with divergent flags) harm onboarding. One CLI is part of the language product (RFC-000 P5).

### 2.2 Why now

All toolchain RFCs need a user-facing contract.

### 2.3 Success metrics

| Metric          | Target                                     |
| --------------- | ------------------------------------------ |
| Discoverability | `aura --help` covers daily verbs           |
| Scriptability   | Stable exit codes + JSON modes             |
| Speed           | `aura check` optimized path (no full link) |

## 3. Goals

- One binary: `aura`.
- Daily workflow verbs with Cargo/Go-like familiarity.
- Consistent global flags (`--verbose`, `--color`, `--dir`).
- Extensibility via subcommands; avoid plugin free-for-all in MVP.

## 4. Non-goals

- Interactive TUI IDE replacement.
- Cross-language task runner (Make/npm scripts platform).
- Guaranteed stable output formatting of human text (JSON is stable).

## 5. Prior art & alternatives

| CLI   | Notes           | Take        |
| ----- | --------------- | ----------- |
| cargo | Subcommand UX   | **Primary** |
| go    | Simple verbs    | Inspiration |
| npm   | Scripts culture | Contrast    |
| git   | Ubiquity        | Help style  |

## 6. Design

### 6.1 Command map

| Command                           | Purpose                                                            |
| --------------------------------- | ------------------------------------------------------------------ |
| `aura new <name>`                 | Scaffold project                                                   |
| `aura init`                       | Manifest in existing dir                                           |
| `aura build`                      | Compile & link                                                     |
| `aura run [path] [-- args…]`      | Build (if needed) + execute bin; args after `--` go to the process |
| `aura check`                      | Typecheck/parse without full link                                  |
| `aura test [path] [-- args…]`     | Build & run tests; same `--` pass-through as `run`                 |
| `aura fmt`                        | Format sources                                                     |
| `aura fix`                        | Apply machine-applicable fixes (later)                             |
| `aura doc`                        | Generate docs (later)                                              |
| `aura add` / `remove`             | Dependencies                                                       |
| `aura update`                     | Update lock within constraints                                     |
| `aura tree`                       | Dep graph                                                          |
| `aura publish`                    | Publish package                                                    |
| `aura clean`                      | Remove target/                                                     |
| `aura version` / `aura toolchain` | Version & install (RFC-013)                                        |

### 6.2 Global flags

```text
--help, -h
--version, -V
--verbose, -v
--quiet, -q
--color auto|always|never
--directory, -C <path>
--offline
```

### 6.3 Output & exit codes

| Code | Meaning                                |
| ---- | -------------------------------------- |
| 0    | Success                                |
| 1    | General / test failure / compile error |
| 2    | CLI usage error                        |
| >2   | Reserved (signals-related)             |

`--format json` on `check`/`test` where supported for tooling. Check diagnostics use
stable `code` values, preserve byte and line/column `span` data, and include an
`operation` field for async/task errors (`await`, `spawn`, `join`, `cancel`, or
channel operations). Pretty diagnostics show the same code and operation label.

Async diagnostic codes are stable: `E-BORROW-ASYNC-ESCAPE` for values crossing an
async/task/channel boundary, `E-ASYNC-TASK-OP` for invalid task operations,
`E-ASYNC-CANCEL` for invalid cancellation state, and `E-ASYNC-CHANNEL-STATE` for
invalid channel state or element operations. Each code includes an actionable note.

### 6.4 Configuration

- Project: `aura.toml`
- User: `~/.aura/config.toml` (proxy, registry, defaults)
- Env: `AURA_*` overrides documented

### 6.5 Formatting (`aura fmt`)

- Deterministic formatter; CI `--check` mode.
- Config subset in `aura.toml` `[fmt]` (line width, etc.).

### 6.6 UX principles

- Prefer sensible defaults over required flags.
- Errors: compiler diagnostics passthrough with summary.
- Never delete `src/` on `clean`.

### 6.7 Examples

```text
aura new hello && cd hello
aura run
aura run . -- flag value
aura test
aura build --release -o hello
aura check --format json
```

### 6.8 Error model / edge cases

| Case                | Behavior                       |
| ------------------- | ------------------------------ |
| Not a project dir   | Error suggesting `init`        |
| Multiple bins `run` | Require `--bin`                |
| Ctrl-C              | Non-zero; cancel build workers |

### 6.9 Compatibility & migration

- Subcommand names stable post-0.1.
- Hidden aliases allowed; deprecated flags warn.

## 7. Open questions

| #   | Question                           | Options           | Owner | Status                                                                          |
| --- | ---------------------------------- | ----------------- | ----- | ------------------------------------------------------------------------------- |
| 1   | `aura pkg` namespace vs flat `add` | flat (`aura add`) | CLI   | **Resolved**                                                                    |
| 2   | Plugin subcommands                 | later             | CLI   | **Deferred** — post-MVP                                                         |
| 3   | Shell completion packaging         |                   | CLI   | **Resolved** — `aura completions <shell>` generates scripts (not separate pkgs) |

## 8. Rationale & trade-offs

Cargo-like flat verbs optimize for daily memory. Single binary matches product story. JSON for machines, human text for humans. Cost: large CLI surface—mitigated by good help and docs.

## 9. Unresolved / future work

- `aura fix`, `aura doc`, `aura bench`
- Interactive `aura add` search
- Watch mode `aura run --watch`

## 10. Security & safety considerations

- `publish` requires explicit auth.
- Commands that execute project code (`run`, `test`) are trusted-project operations.
- Config file permissions documented on multi-user systems.

## 11. Implementation plan (optional)

| Phase | Scope               | Exit criteria |
| ----- | ------------------- | ------------- |
| L0    | new/build/run/check | Hello path    |
| L1    | test/fmt            | CI usable     |
| L2    | add/update/publish  | Package path  |

## 12. References

- Cargo command reference
- RFC-005, RFC-008, RFC-011, RFC-013

---

## Changelog

| Date       | Author | Change                                                                           |
| ---------- | ------ | -------------------------------------------------------------------------------- |
| 2026-07-16 |        | Defer plugin cmds; lock `aura completions <shell>`                               |
| 2026-07-16 |        | Status → **Accepted** — Review: command map matches shipped check/build/run/test |
| 2026-07-16 |        | Note shipped check/build/run/test/emit-c                                         |
| 2026-07-15 |        | Initial skeleton                                                                 |
| 2026-07-15 |        | Solid draft: command map, exit codes                                             |
| 2026-07-15 |        | Lock flat package commands                                                       |
