# Phase 3 — Runtime and ABI Stabilization (v1.0)

_Last updated: 2026-04-09_

## Goal

Harden the embedded runtime boundary so codegen can depend on a stable C ABI for allocations, strings, arrays, object dispatch, and exceptions.

## Scope

- Keep the runtime as a static library embedded into the final executable.
- Document the ABI boundary for generated code.
- Preserve current exception/unwinding behavior.
- Keep object layout and helper calls understandable from docs alone.

## Runtime ABI Surface

The public C header lives at `runtime/include/aura_rt.h`. Codegen currently depends on these exported symbols:

- Memory and object lifetime: `aura_alloc`, `aura_retain`, `aura_release`
- Exception handling: `aura_try_begin`, `aura_try_end`, `aura_current_exception`, `aura_has_active_handler`, `aura_throw`
- Strings and printing: `aura_string_new_utf8`, `aura_string_concat`, `aura_println`
- Primitive formatting helpers: `aura_i32_to_string`, `aura_i64_to_string`, `aura_f32_to_string`, `aura_f64_to_string`, `aura_bool_to_string`
- Fatal error handling: `aura_panic`

Stable for v1.0 means the compiler may rely on these function names, signatures, and the header types they reference. Provisional runtime helpers should stay out of codegen until they are documented here and in `docs/ARCHITECTURE.md`.

## Object Layout and Dispatch

- `AuraObject` starts with a vtable pointer and a reference count.
- `AuraString` embeds `AuraObject` as its header, then stores `len` and the UTF-8 data pointer.
- `AuraHandlerFrame` currently carries the previous frame link, catch entry, cleanup stack pointer, and the jump buffer used by the runtime's unwinding path.
- Codegen loads the vtable pointer from the object header, then indexes the vtable by method slot for dynamic dispatch.
- The layout above is the only part codegen should assume; any deeper runtime bookkeeping remains an implementation detail unless the header is updated in the same change.

## TODO

- [x] Document the runtime entry points currently used by codegen. (done 2026-04-09)
- [x] Define which runtime APIs are stable and which are provisional. (done 2026-04-09)
- [x] Describe object header fields and dispatch assumptions in one place. (done 2026-04-09)
- [x] Add tests that validate runtime ABI usage from compiler output. (done 2026-04-09)
- [x] Capture any header or layout changes in `runtime/include/`. (done 2026-04-09)

## Acceptance

- [x] The compiler/runtime contract is explicit enough to keep backend and runtime changes decoupled. (done 2026-04-09)
- [x] Runtime ABI changes are reflected in architecture docs before or with implementation. (done 2026-04-09)
- [x] Exception and cleanup semantics remain aligned with the v0 contract. (done 2026-04-09)
