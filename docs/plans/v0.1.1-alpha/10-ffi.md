# Workstream 10: Extended FFI

Owner: Compiler + Runtime. Scope: 6 tasks.

## F1. FFI declaration model

**Objective:** Represent foreign symbols and ABI requirements safely.
**Checklist:**

- [x] Define foreign functions, libraries, calling conventions, target guards,
      link settings, and ABI metadata for `@foreign(...) extern "C" fun ...`.
- [x] Validate declarations before code generation.
- [x] Preserve source spans and actionable errors.
      **Acceptance:** Invalid declarations fail without attempting unsafe linking.
      **Verification:** Run parser, sema, target, and ABI validation fixtures.
      **Dependencies:** A1–A3, B1–B5.

**Implementation status (F1):** Complete for the alpha declaration boundary.
Foreign declarations are represented in the AST and package loader, accept only
the C calling convention, require an explicit library/target/link/ABI
descriptor, and are rejected by sema before code generation. The compiler's
actual native matrix is enforced (`native`, or the matching Linux/macOS host
triple). Calls, primitive lowering, and foreign linking remain F2 behavior.

## F2. Primitive calls

**Objective:** Call foreign functions with supported primitive values.
**Checklist:**

- [x] Define integer, boolean, string-handle, and void conventions.
- [x] Generate target-aware declarations and linking behavior.
- [x] Map explicitly declared primitive status failures to documented Aura
      outcomes.
      **Acceptance:** Primitive calls work consistently on supported hosts.
      **Verification:** Run native calls, missing-symbol, wrong-signature, and error
      fixtures on Linux/macOS.
      **Dependencies:** F1, P6–P7.

**Implementation status (F2, bounded alpha slice):** Primitive C calls are
lowered for `Int` (`int64_t`), `Bool` (`bool`), `String` (`const char *`), and
`Unit` (`void`). String arguments and results are borrowed handles for the
duration of the call; codegen never frees a foreign String result. Foreign
symbols are emitted verbatim with an `extern` prototype. `target = "native"`
and the host Linux/macOS matrix are enforced by F1. Dynamic libraries use
`-lNAME`; static libraries use `-Wl,-Bstatic -lNAME -Wl,-Bdynamic` on Linux
and `-Wl,-force_load,libNAME.a` on macOS. Additional library search paths are
explicit `CompileOptions::foreign_library_path` entries. A declaration may
opt into `failure = "status"` for an `Int` return; codegen passes the foreign
integer through `aura_ffi_map_error`, yielding the documented bounded outcome
codes (`OK`, `CANCELLED`, `INVALID`, `NOT_FOUND`, `PERMISSION`, `UNAVAILABLE`,
`TIMEOUT`, or `FOREIGN_ERROR`). Undecorated primitive returns remain ordinary
values, and linker failures remain deterministic build diagnostics. Floating-
point, callbacks, pointers, and structured values remain deferred to F4–F5;
the bounded owned-string/primitive-array ABI is F3.

**Verification:** `aura-codegen` has a local static C fixture covering Int,
Bool, borrowed String, Unit, and `failure = "status"` calls; it compiles and
runs on the native host, while sema tests reject the status convention on a
non-`Int` return.

## F3. Owned strings and arrays

**Objective:** Transfer structured values across the FFI boundary without leaks.
**Checklist:**

- [x] Define encoding, layout, length, capacity, ownership, and destruction.
- [x] Support explicit borrow/copy/transfer operations only where contracted.
- [x] Root values while foreign code can access them.
      **Acceptance:** Success and failure paths release exactly once.
      **Verification:** Run nested, empty, large, GC, and sanitizer cases.
      **Dependencies:** F2, A3.

**Implementation status (F3, bounded alpha slice):** `runtime/aura_ffi.h`
defines explicit borrowed and owned String records and array records with
`len`, `cap`, `elem_size`, and a fixed primitive element kind. Borrow never
allocates; copy creates independent malloc-backed storage; transfer consumes
only malloc-compatible storage and is one-shot. Destruction is null-safe and
idempotent. Array copy/transfer is limited to bytes, `int64_t`, and one-byte
booleans; String-element deep-copy, arbitrary destructors, pointers, and
callbacks remain outside this slice. `AuraFfiRootGuard` roots a GC slot only
for a synchronous foreign-call window and never across await, task, or
callback boundaries. The strict fixture covers nested cleanup, empty and large
arrays, transfer, GC collection while rooted, and sanitizer-friendly
double-destroy paths.

## F4. Foreign pointers

**Objective:** Represent opaque external resources with explicit lifetime rules.
**Checklist:**

- [x] Define pointer creation, nullability, pinning, release, and invalidation.
- [x] Prevent accidental dereference or use-after-release in Aura code.
- [x] Define task, await, channel, and callback crossing rules.
      **Acceptance:** Invalid pointer lifetimes are rejected or reported deterministically.
      **Verification:** Run null, double-release, early-release, GC, and cancellation
      fixtures.
      **Dependencies:** F3, A3.

**Implementation status (F4, bounded alpha slice):** `runtime/aura_ffi.h`
exposes separate non-null and nullable opaque-handle constructors. A handle
never exposes its resource directly: a checked pin token is required to obtain
the borrowed resource for one synchronous operation window. Release and
invalidation immediately tombstone the handle, reject stale aliases and
double-release, and defer the destructor until outstanding pins are returned.
Tombstones are explicitly destroyed after release; destroying an active handle
or one with pins is rejected. Handle values are rejected at task, await,
channel, and callback boundaries; this matches the existing sema rule that
borrowed values cannot cross those asynchronous/ownership boundaries.

**Verification:** `runtime/tests/ffi_handles.c` is compiled with strict C11
warnings and exercises nullable construction, null and boundary behavior,
double release, stale use after release, early destruction, pinning, deferred
cleanup, and invalidated-handle cleanup. Callback implementation remains F5.

## F5. Callbacks and errors

**Objective:** Make foreign callbacks and failures safe across execution models.
**Checklist:**

- [x] Define callback ownership, thread/task affinity, and shutdown behavior.
- [x] Map foreign error codes/exceptions into Aura outcomes.
- [x] Prevent callbacks from observing destroyed frames or values.
      **Acceptance:** Callback lifetime and failure behavior are documented and tested.
      **Verification:** Run callback, re-entry, cancellation, and foreign-error cases.
      **Dependencies:** F4, S1–S6.

**Implementation status (F5, bounded alpha slice):** `runtime/aura_ffi.h`
defines a synchronous callback registration whose environment is owned by the
registration and destroyed exactly once by deregistration or shutdown. Each
registration retains an explicit owner frame and task id; delivery from a
different task, or through await/channel/callback boundaries, is rejected.
Re-entry while a callback is dispatching returns `AURA_FFI_BUSY`. Frame
invalidation rejects later delivery, and frame destruction is refused while a
registration still retains it, preventing a destroyed callback frame from
being observed. Foreign return codes 0–6 map to documented Aura outcomes;
unknown codes map to `FOREIGN_ERROR`.

**Verification:** `runtime/tests/ffi_callbacks.c` is compiled with strict C11
warnings and exercises environment lifetime, task/await affinity rejection,
re-entry, frame invalidation/destruction prevention, idempotent shutdown, and
foreign timeout/unknown-error mapping. This is a single-threaded bounded
runtime fixture; cross-host callback acceptance, concurrent callback delivery,
and exception-object translation remain outside F5.

## F6. FFI acceptance and sanitizers

**Objective:** Prove the extended FFI contract on supported targets.
**Checklist:**

- [x] Add ABI mismatch, linker, ownership, callback, and sanitizer fixtures.
  - [x] Runtime FFI ABI identity mismatch is rejected before user `main`, with
        exit status 78 and both ABI identities in stderr.
  - [x] Invalid linker flavor is surfaced as a deterministic codegen compile
        error, with the emitted C source retained for diagnosis and no output
        executable reported or left behind.
  - [x] Ownership, callback, and sanitizer fixtures.
- [ ] Run native acceptance on Linux and macOS.
  - [x] Run the Linux native matrix with
        `scripts/ffi-regression.sh`; it covers owned values, opaque handles,
        callbacks, ASAN/UBSAN, and the compiler primitive-call fixture.
  - [ ] Run the same acceptance matrix on macOS.
- [x] Verify no unowned opaque foreign handle crosses task, await, channel,
      or callback boundaries; general foreign values remain bounded to the
      synchronous FFI contract.
      **Acceptance:** FFI failures are safe, diagnosed, and reproducible.
      **Verification:** Execute the FFI stage from clean checkouts and release builds.
      **Dependencies:** F1–F5, A8, P8.

**Bounded evidence:** The Linux fixture set covers ABI mismatch, invalid linker,
owned-value cleanup, opaque-handle lifetime, callback affinity/re-entry, and
ASAN/UBSAN/LSAN paths. Native macOS acceptance and compiler-level proof that
no unowned foreign value crosses an await/task boundary remain open.
