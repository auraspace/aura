# Workstream 10: Extended FFI

Owner: Compiler + Runtime. Scope: 6 tasks.

## F1. FFI declaration model

**Objective:** Represent foreign symbols and ABI requirements safely.
**Checklist:**

- [ ] Define foreign functions, libraries, calling conventions, target guards,
      link settings, and ABI metadata.
- [ ] Validate declarations before code generation.
- [ ] Preserve source spans and actionable errors.
      **Acceptance:** Invalid declarations fail without attempting unsafe linking.
      **Verification:** Run parser, sema, target, and ABI validation fixtures.
      **Dependencies:** A1–A3, B1–B5.

## F2. Primitive calls

**Objective:** Call foreign functions with supported primitive values.
**Checklist:**

- [ ] Define integer, boolean, floating, string-handle, and void conventions.
- [ ] Generate target-aware declarations and linking behavior.
- [ ] Map foreign failures to documented Aura outcomes.
      **Acceptance:** Primitive calls work consistently on supported hosts.
      **Verification:** Run native calls, missing-symbol, wrong-signature, and error
      fixtures on Linux/macOS.
      **Dependencies:** F1, P6–P7.

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

  Proven slice: the generated executable rejects a runtime whose FFI ABI
  component differs, before user `main` runs, with deterministic exit status 78
  and both ABI identities in stderr. Linker, ownership, callback, and sanitizer
  fixtures remain open.
- [ ] Run native acceptance on Linux and macOS.
- [ ] Verify no unowned foreign value crosses task or await boundaries.
      **Acceptance:** FFI failures are safe, diagnosed, and reproducible.
      **Verification:** Execute the FFI stage from clean checkouts and release builds.
      **Dependencies:** F1–F5, A8, P8.
