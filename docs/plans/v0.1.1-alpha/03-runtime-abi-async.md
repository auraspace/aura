# Workstream 03: Runtime ABI and Async State Machine

Owner: Compiler + Runtime. Scope: 8 tasks.

This workstream defines the ownership and suspension contract used by spawn,
channels, async I/O, HTTP, race instrumentation, and FFI.

## A1. ABI version metadata

**Objective:** Detect compiler/runtime contract mismatches before execution.

**Implementation status:** Complete for the shipped C runtime. The compiler
embeds one contract identity covering task, value, exception, channel, GC, I/O,
and FFI calls in every generated artifact. The runtime exposes the available
identity and generated `aura_main` rejects a mismatch before creating task
state or calling user code (exit status 78). Patch-level runtime fixes must
preserve the identity; any layout, ownership, or calling-convention change
must publish a new ABI identity/version. Static linking remains the default,
so this check also protects builds that select an alternate `AURA_RUNTIME`.

**Checklist:**

- [x] Version task, value, exception, channel, GC, I/O, and FFI ABI rules.
- [x] Embed ABI identity in generated artifacts and runtime metadata.
- [x] Define compatibility policy for patch and breaking changes.
- [x] Produce a diagnostic identifying expected and available versions.

**Acceptance:** An incompatible artifact fails before unsafe execution.

**Verification:** Run matching fixtures, then intentionally mismatch versions.

**Dependencies:** B2.

## A2. Task frame layout

**Objective:** Define the complete state retained while an async operation is
suspended.

**Checklist:**

- [ ] Represent state, resume location, pending operation, captures, locals,
      result, error, cancellation, and completion state.
- [ ] Define initialization, polling, completion, and destruction transitions.
- [ ] Make frame layout inspectable in compiler/runtime tests.
- [ ] Define immediate completion and repeated polling behavior.

**Acceptance:** Every legal async path has a defined frame state and lifecycle.

**Verification:** Exercise no-await, ready, pending, repeated-poll, and
completion fixtures.

**Dependencies:** A1.

## A3. Ownership and GC-root rules

**Objective:** Prevent invalid values from crossing suspension or task boundaries.

**Checklist:**

- [ ] Classify values as owned, borrowed, pinned, shared, or transferred.
- [ ] Define root/mark/drop behavior for locals, captures, results, and payloads.
- [ ] Reject borrowed values that outlive their legal scope.
- [ ] Define foreign-pointer and external-resource treatment.

**Acceptance:** No unrooted or borrowed value survives await, spawn, channel
send, returned task, or callback boundaries.

**Verification:** Run positive ownership and negative lifetime fixtures under
forced GC.

**Dependencies:** A2.

## A4. Async state-machine lowering

**Objective:** Lower async functions into explicit states instead of relying on
backend-specific control flow.

**Checklist:**

- [ ] Identify suspension points and assign deterministic state IDs.
- [ ] Hoist live locals and owned values into frame storage.
- [ ] Generate resume and completion edges with source spans.
- [ ] Preserve return, throw, and cleanup semantics.

**Acceptance:** Empty and no-await async functions use the same representation.

**Verification:** Inspect state dumps and execute async positive/negative cases.

**Dependencies:** A2, A3.

## A5. Single await suspension

**Objective:** Make one pending operation suspend and later resume correctly.

**Checklist:**

- [ ] Distinguish ready, pending, failed, and cancelled poll results.
- [ ] Save all values live across await.
- [ ] Resume exactly once when the operation completes.
- [ ] Prevent frame destruction while pending.

**Acceptance:** Immediate and delayed completion produce the same result.

**Verification:** Use deterministic scheduler tests for both completion orders and
inspect drop counts.

**Dependencies:** A4.

## A6. Multiple awaits and drops

**Objective:** Support multiple suspension points with correct cleanup.

**Checklist:**

- [ ] Generate distinct states and resume locations for each await.
- [ ] Track values whose lifetime crosses only some states.
- [ ] Drop values exactly once on success, error, cancellation, and panic.
- [ ] Preserve source mapping for every transition.

**Acceptance:** Strings, arrays, classes, and nested results remain valid across
multiple awaits and release on every exit path.

**Verification:** Run multi-await, forced-GC, cancellation, and failure fixtures
under sanitizers.

**Dependencies:** A5.

## A7. Async exception outcomes

**Objective:** Make async success, exception, and cancellation observable and
composable by callers.

**Checklist:**

- [ ] Define successful values and failure payload representation.
- [ ] Preserve exception source spans through suspension.
- [ ] Run cleanup before publishing an outcome.
- [ ] Define an exception during cancellation.

**Acceptance:** Consumers distinguish success, failure, and cancellation without
backend-specific details.

**Verification:** Execute outcome fixtures through direct await and task join.

**Dependencies:** A6.

## A8. ABI sanitizer suite

**Objective:** Prove frame and value ownership under hostile lifecycle conditions.

**Checklist:**

- [ ] Exercise pending frames during forced GC.
- [ ] Exercise cancellation, repeated polling, dropped handles, and failures.
- [ ] Run memory and undefined-behavior sanitizers where supported.
- [ ] Record reproducible seeds and minimized failures.

**Acceptance:** No use-after-free, double-drop, invalid root, or leaked frame is
present in the mandatory suite.

**Verification:** Run the ABI stage on every supported native host.

**Dependencies:** A1–A7.
