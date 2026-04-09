# Phase 4 — Backend and Linking (v1.0)

_Last updated: 2026-04-09_

## Goal

Keep backend selection and linking pluggable enough for another target or backend to land without rewriting the CLI pipeline.

## Scope

- Make backend capability checks explicit.
- Keep LLVM as the current MVP default.
- Keep Cranelift as a placeholder until intentionally promoted.
- Keep linking logic isolated in `aura-link`.

## Backend Selection and Capability Reporting

`aurac` chooses a backend in two steps:

1. Parse `--backend=...`, defaulting to LLVM when the flag is omitted.
2. Ask `aura-codegen` for the selected backend's capabilities before building or emitting debug output.

Current capability reporting is intentionally small:

- LLVM supports object emission plus `--emit=llvm` and `--emit=asm`.
- Cranelift is still a placeholder backend and supports object emission only.
- Unsupported emit modes should fail before backend construction with a clear error message.

## Linker Strategy

- On `aarch64-apple-darwin`, `aura-link` should continue to drive the system toolchain (`clang` / `ld`) for Mach-O output.
- Future targets should select their linker strategy from target metadata instead of hardcoding host-specific branches in `aurac`.
- Linking should stay behind `aura-link` so the CLI only passes object paths, runtime archives, and target descriptors.

## TODO

- [x] Define backend capability reporting in `aura-codegen`. (done 2026-04-09)
- [x] Document how `aurac` chooses a backend for a given target. (done 2026-04-09)
- [x] Add a clear fallback/error path for unsupported emit modes. (done 2026-04-09)
- [x] Add a linker strategy note for macOS versus future targets. (done 2026-04-09)
- [x] Add at least one test that exercises backend selection or rejection logic. (done 2026-04-09)

## Acceptance

- [x] Backend choice is explicit and testable. (done 2026-04-09)
- [x] Linker behavior remains isolated from code generation details. (done 2026-04-09)
- [x] The docs describe how to add a backend without touching unrelated stages. (done 2026-04-09)
