# Phase 5 — Runtime ABI Skeleton (parallelizable)

_Last updated: 2026-04-07_

## Goal

Define a stable C ABI boundary and a minimal runtime static library.

## TODO

- [x] Add `runtime/include/` header(s) that define ABI used by codegen (done 2026-04-08)
- [x] Implement runtime `staticlib` build target (done 2026-04-08)
- [x] Add minimal runtime entrypoints: (done 2026-04-08)
  - [x] `aura_alloc(size, align)`
  - [x] (optional) `aura_retain(ptr)` / `aura_release(ptr)`
  - [x] `aura_string_new_utf8(ptr, len)`
  - [x] `aura_println(str)`
  - [x] `aura_panic(msg_ptr, msg_len)`
- [x] Document ABI signatures and versioning expectations for MVP (done 2026-04-08)

## Acceptance

- [x] Runtime builds as a static lib and is linkable into a minimal executable (done 2026-04-08)
- [x] ABI header is the single source of truth for runtime calls from codegen (done 2026-04-08)

