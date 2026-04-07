# Phase 6 — Codegen + Link (aarch64-apple-darwin)

_Last updated: 2026-04-07_

## Goal

Compile MIR into a Mach-O object and link it with runtime into a single executable.

## TODO

- [ ] Pick backend for MVP (Cranelift or LLVM) and define backend interface
- [ ] Emit `.o` for `aarch64-apple-darwin`
- [ ] Link via `clang`/`ld` with runtime staticlib
- [ ] Add inspection outputs: `--emit=obj|asm`
- [ ] Add first E2E compile+run test for `examples/hello`

## Acceptance

- [ ] `aurac run examples/hello` prints expected output and exits 0
- [ ] `--emit=obj|asm` produces files for debugging

