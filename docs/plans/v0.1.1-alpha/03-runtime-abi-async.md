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

**Implementation status:** Complete for the C runtime frame ABI. Frames retain
locals, captures, pending operation, resume state, result, error, cancellation,
and terminal state. Every owned storage slot is released exactly once during
replacement or frame destruction; the deterministic executor covers immediate,
pending, repeated-poll, and completion transitions.

**Checklist:**

- [x] Represent state, resume location, pending operation, captures, locals,
      result, error, cancellation, and completion state.
- [x] Define initialization, polling, completion, and destruction transitions.
- [x] Make frame layout inspectable in compiler/runtime tests.
- [x] Define immediate completion and repeated polling behavior.

**Acceptance:** Every legal async path has a defined frame state and lifecycle.

**Verification:** Exercise no-await, ready, pending, repeated-poll, and
completion fixtures.

**Dependencies:** A1.

## A3. Ownership and GC-root rules

**Objective:** Prevent invalid values from crossing suspension or task boundaries.

**Implementation status:** Complete for the current compiler/runtime boundary.
The sema pass rejects borrowed values across await/spawn/channel boundaries;
runtime frame storage classifies owned, borrowed, pinned, shared, and
transferred values, roots non-borrowed storage while suspended, and releases
its root and destructor exactly once.

**Checklist:**

- [x] Classify values as owned, borrowed, pinned, shared, or transferred.
- [x] Define root/mark/drop behavior for locals, captures, results, and payloads.
- [x] Reject borrowed values that outlive their legal scope.
- [x] Define foreign-pointer and external-resource treatment.

**Acceptance:** No unrooted or borrowed value survives await, spawn, channel
send, returned task, or callback boundaries.

**Verification:** Run positive ownership and negative lifetime fixtures under
forced GC.

**Dependencies:** A2.

## A4. Async state-machine lowering

**Objective:** Lower async functions into explicit states instead of relying on
backend-specific control flow.

**Implementation status:** Bounded compiler slice complete. The AST walks async
bodies in deterministic lexical order, reserves state `0` for entry, assigns
states `1..N` to `await` points, and codegen emits stable kind/source-span
metadata. Live-local hoisting and executable resume-edge lowering remain in
A5–A6.

**Checklist:**

- [x] Identify `await` suspension points and assign deterministic state IDs
      with source-span metadata (bounded compiler slice).
- [ ] Hoist live locals and owned values into frame storage.
- [ ] Generate resume and completion edges with source spans.
- [ ] Preserve return, throw, and cleanup semantics.

**Acceptance:** Empty and no-await async functions use the same representation.

**Verification:** Inspect state dumps and execute async positive/negative cases.

**Dependencies:** A2, A3.

## A5. Single await suspension

**Objective:** Make one pending operation suspend and later resume correctly.
**Implementation status:** Immediate-completion and non-waiting pending
continuation paths are complete through runtime polling and executor requeue.
The bounded codegen test for one `await` with an `Int` and a `String` used after
the await confirms only the current boundary: the input task is retained in
frame data, while those locals remain in the ordinary helper and are not
hoisted. Operation wakeup, live-local hoisting, and full async I/O suspension
remain open.

**Checklist:**

- [x] Distinguish ready, pending, failed, and cancelled poll results.
- [ ] Save all values live across await.
- [x] Resume exactly once for non-waiting pending frames when the operation
      completes; waiter-driven resumption remains open.
- [x] Prevent executor-owned frame destruction while pending.

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

- [x] Define successful values and failure payload representation for the
      bounded frame ABI.
- [ ] Preserve exception source spans through suspension.
- [x] Run cleanup before publishing an outcome in the bounded executor ABI.
- [ ] Define an exception during cancellation.

**Acceptance:** Consumers distinguish success, failure, and cancellation without
backend-specific details.

**Verification:** Execute outcome fixtures through direct await and task join.

**Bounded evidence (2026-07-22):**
`runtime/tests/task_outcomes.c` drives the existing single-threaded frame
ABI through direct polling and `aura_task_executor_join`. It distinguishes an
owned integer result, an owned error payload with a stable numeric source ID,
and cancellation. The fixture records that success/error cleanup runs before
join publishes the borrowed outcome, while runtime cancellation releases
pending/capture ownership before returning `AURA_TASK_CANCELLED` and never
invokes the poll callback. This is a bounded C ABI result; it does not claim
compiler suspension, source file/line spans, or nested exception chains.

**Status note:** The bounded successful-value/failure-payload and
cleanup-before-outcome items are complete. Source-span preservation and an
exception raised during cancellation remain open until typed compiler outcome
lowering exists.

**Dependencies:** A6.

## A8. ABI sanitizer suite

**Objective:** Prove frame and value ownership under hostile lifecycle conditions.

**Checklist:**

- [x] Exercise pending frames during forced GC.
- [x] Exercise cancellation, repeated polling, dropped handles, and failures.
- [x] Run memory and undefined-behavior sanitizers where supported.
- [ ] Record reproducible seeds and minimized failures.

**Bounded evidence (2026-07-22):**
`runtime/tests/task_frame_sanitizer.c` covers a pending frame retaining a GC
capture across `aura_gc_collect`, repeated direct polling, executor
cancellation, a dropped pending handle cleaned by executor shutdown, and a
failed frame whose error payload is observed and released. This is a C ABI
fixture for the existing single-threaded frame/executor APIs; it does not claim
full async state-machine lowering or delayed wakeup support. The fixture is
run with ASAN/UBSAN and, when supported by the host toolchain, LSAN.

**Acceptance:** No use-after-free, double-drop, invalid root, or leaked frame is
present in the mandatory suite.

**Verification:** Run the ABI stage on every supported native host.

**Dependencies:** A1–A7.
