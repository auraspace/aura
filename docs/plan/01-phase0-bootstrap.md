# Phase 0 — Repo Bootstrap

_Last updated: 2026-04-07_

## Goal

Turn the repo from “docs-only” into a Rust workspace that matches `docs/FOLDER_STRUCTURE.md`.

## TODO

- [ ] Add root `Cargo.toml` workspace
- [ ] Create crates: `aurac`, `aura-driver`, `aura-span`, `aura-diagnostics`, `aura-lexer`, `aura-parser`, `aura-ast`
- [ ] Add minimal CLI skeleton: `aurac build|check|run` (help output stable)
- [ ] Add placeholder dirs: `runtime/`, `examples/`, `tests/`

## Acceptance

- [ ] `cargo check` succeeds on macOS
- [ ] `aurac --help` runs and shows subcommands

