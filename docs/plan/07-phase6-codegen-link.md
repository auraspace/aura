# Phase 6 — Codegen + Link (aarch64-apple-darwin)

_Last updated: 2026-04-08_

## Goal

Compile MIR into a Mach-O object and link it with runtime into a single executable.

## TODO

- [x] Pick backend for MVP (Cranelift or LLVM) and define backend interface (done 2026-04-08)
- [x] Emit `.o` for `aarch64-apple-darwin` (done 2026-04-08)
- [x] Link via `clang`/`ld` with runtime staticlib (done 2026-04-08)
- [x] Add inspection outputs: `--emit=obj|asm` (done 2026-04-08)
- [x] Add first E2E compile+run test for `examples/hello` (done 2026-04-08)

## Acceptance

- [x] `aurac run examples/hello` prints expected output and exits 0 (done 2026-04-08)
- [x] `--emit=obj|asm` produces files for debugging (done 2026-04-08)
