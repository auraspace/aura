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
- [ ] Map foreign failures to documented Aura outcomes.
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
explicit `CompileOptions::foreign_library_path` entries. Floating-point,
callbacks, owned/transfer strings, arrays, pointers, and structured failure
outcomes remain deferred to F3–F5.

**Verification:** `aura-codegen` has a local static C fixture covering Int,
Bool, borrowed String, and Unit calls; it compiles and runs on the native host.

## F3. Owned strings and arrays

**Objective:** Transfer structured values across the FFI boundary without leaks.
**Checklist:**

- [ ] Define encoding, layout, length, capacity, ownership, and destruction.
- [ ] Support explicit borrow/copy/transfer operations only where contracted.
- [ ] Root values while foreign code can access them.
      **Acceptance:** Success and failure paths release exactly once.
      **Verification:** Run nested, empty, large, GC, and sanitizer cases.
      **Dependencies:** F2, A3.

## F4. Foreign pointers

**Objective:** Represent opaque external resources with explicit lifetime rules.
**Checklist:**

- [ ] Define pointer creation, nullability, pinning, release, and invalidation.
- [ ] Prevent accidental dereference or use-after-release in Aura code.
- [ ] Define task, await, channel, and callback crossing rules.
      **Acceptance:** Invalid pointer lifetimes are rejected or reported deterministically.
      **Verification:** Run null, double-release, early-release, GC, and cancellation
      fixtures.
      **Dependencies:** F3, A3.

## F5. Callbacks and errors

**Objective:** Make foreign callbacks and failures safe across execution models.
**Checklist:**

- [ ] Define callback ownership, thread/task affinity, and shutdown behavior.
- [ ] Map foreign error codes/exceptions into Aura outcomes.
- [ ] Prevent callbacks from observing destroyed frames or values.
      **Acceptance:** Callback lifetime and failure behavior are documented and tested.
      **Verification:** Run callback, re-entry, cancellation, and foreign-error cases.
      **Dependencies:** F4, S1–S6.

## F6. FFI acceptance and sanitizers

**Objective:** Prove the extended FFI contract on supported targets.
**Checklist:**

- [ ] Add ABI mismatch, linker, ownership, callback, and sanitizer fixtures.
  - [x] Runtime FFI ABI identity mismatch is rejected before user `main`, with
        exit status 78 and both ABI identities in stderr.
  - [x] Invalid linker flavor is surfaced as a deterministic codegen compile
        error, with the emitted C source retained for diagnosis and no output
        executable reported or left behind.
  - [ ] Ownership, callback, and sanitizer fixtures.
- [ ] Run native acceptance on Linux and macOS.
- [ ] Verify no unowned foreign value crosses task or await boundaries.
      **Acceptance:** FFI failures are safe, diagnosed, and reproducible.
      **Verification:** Execute the FFI stage from clean checkouts and release builds.
      **Dependencies:** F1–F5, A8, P8.
