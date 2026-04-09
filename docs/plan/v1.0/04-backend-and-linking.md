# Phase 4 — Backend and Linking (v1.0)

_Last updated: 2026-04-09_

## Goal

Keep backend selection and linking pluggable enough for another target or backend to land without rewriting the CLI pipeline.

## Scope

- Make backend capability checks explicit.
- Keep LLVM as the current MVP default.
- Keep Cranelift as a placeholder until intentionally promoted.
- Keep linking logic isolated in `aura-link`.

## TODO

- [ ] Define backend capability reporting in `aura-codegen`.
- [ ] Document how `aurac` chooses a backend for a given target.
- [ ] Add a clear fallback/error path for unsupported emit modes.
- [ ] Add a linker strategy note for macOS versus future targets.
- [ ] Add at least one test that exercises backend selection or rejection logic.

## Acceptance

- [ ] Backend choice is explicit and testable.
- [ ] Linker behavior remains isolated from code generation details.
- [ ] The docs describe how to add a backend without touching unrelated stages.
