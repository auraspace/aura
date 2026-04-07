# Phase 0 — Repo Bootstrap

_Last updated: 2026-04-07_

## Goal

Turn the repo from “docs-only” into a Rust workspace that matches `docs/FOLDER_STRUCTURE.md`.

## TODO

- [x] Add root `Cargo.toml` workspace (done 2026-04-07)
- [x] Create crates: `aurac`, `aura-driver`, `aura-span`, `aura-diagnostics`, `aura-lexer`, `aura-parser`, `aura-ast` (done 2026-04-07)
- [x] Add minimal CLI skeleton: `aurac build|check|run` (help output stable) (done 2026-04-07)
- [x] Add placeholder dirs: `runtime/`, `examples/`, `tests/` (done 2026-04-07)

## Acceptance

- [x] `cargo check` succeeds on macOS (done 2026-04-07)
- [x] `aurac --help` runs and shows subcommands (done 2026-04-07)
