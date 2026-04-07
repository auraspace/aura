# Phase 5 — Runtime ABI Skeleton (parallelizable)

_Last updated: 2026-04-07_

## Goal

Define a stable C ABI boundary and a minimal runtime static library.

## TODO

- [ ] Add `runtime/include/` header(s) that define ABI used by codegen
- [ ] Implement runtime `staticlib` build target
- [ ] Add minimal runtime entrypoints:
  - [ ] `aura_alloc(size, align)`
  - [ ] (optional) `aura_retain(ptr)` / `aura_release(ptr)` if ARC chosen early
  - [ ] `aura_string_new_utf8(ptr, len)`
  - [ ] `aura_println(str)` (or `print/println` surface lowered to ABI)
  - [ ] `aura_panic(msg_ptr, msg_len)`
- [ ] Document ABI signatures and versioning expectations for MVP

## Acceptance

- [ ] Runtime builds as a static lib and is linkable into a minimal executable
- [ ] ABI header is the single source of truth for runtime calls from codegen

