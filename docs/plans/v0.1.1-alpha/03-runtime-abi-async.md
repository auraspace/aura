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

**Implementation status:** The compiler now has an executable straight-line
single-await slice, including the direct `return await task` spelling. It
reserves state `0` for entry, assigns state `1` to the await point, stores live
`Int`/`String` locals in frame data, and uses the runtime parent-child waiter
list to resume the parent when the child reaches a terminal state. Branches,
loops, multiple awaits, and richer owned values remain in A5–A6.

**Checklist:**

- [x] Identify `await` suspension points and assign deterministic state IDs
      with source-span metadata (bounded compiler slice).
- [x] Hoist live `Int`/`String` locals into frame storage for the bounded
      straight-line single-await slice; full control-flow coverage remains open.
- [x] Generate bounded completion/error/cancellation edges for the current
      no-await frame representation with source-ID metadata; resumable await
      edges remain open.
- [x] Preserve return, throw, and cleanup semantics for the bounded no-await
      representation; live values across suspension remain open.

**Acceptance:** Empty and no-await async functions use the same representation.

**Verification:** Inspect state dumps and execute async positive/negative cases.

**Dependencies:** A2, A3.

## A5. Single await suspension

**Objective:** Make one pending operation suspend and later resume correctly.
**Implementation status:** A straight-line single-await lowering now polls a
child frame, registers a parent-child wait when the child is pending, and
resumes exactly once when the runtime wakes the parent. `Int` and `String`
locals used after that await are retained in frame data and cleaned up by the
frame destroy hook. Control-flow partitioning, richer owned values, and full
async I/O operation wiring remain open.

**Checklist:**

- [x] Distinguish ready, pending, failed, and cancelled poll results.
- [x] Save live `Int`/`String` locals across one bounded straight-line await;
      arrays, classes, and control-flow-sensitive locals remain open.
- [x] Resume exactly once for non-waiting pending frames when the operation
      completes; adapter-owned waiting-token registration and clear-before-wake
      resumption are covered by the bounded runtime fixture. Generated await
      operation wiring remains open.
- [x] Prevent executor-owned frame destruction while pending.

**Acceptance:** Immediate and delayed completion produce the same result.

**Verification:** Use deterministic scheduler tests for both completion orders and
inspect drop counts.

**Dependencies:** A4.

## A6. Multiple awaits and drops

**Objective:** Support multiple suspension points with correct cleanup.
**Implementation status:** Straight-line lowering is executable for an
arbitrary number of `Task<Int>` await locals in the current value domain. The
legacy two/three-await helper remains covered, while the general path derives
one resume state and child slot per await; locals initialized between awaits
are stored only after the earlier child completes, and owned String locals are
released exactly once by the frame destroy hook. Branches, loops,
arrays/classes, typed failures, and panic unwinding remain open.

**Checklist:**

- [x] Generate distinct states and resume locations for each await in a
      straight-line sequence (including the bounded two/three-await fixtures).
- [x] Track Int/String values initialized before and between awaits;
      control-flow-sensitive and richer owned values remain open.
- [x] Drop owned String locals exactly once on frame destruction
      after success, error, or cancellation; panic/unwinding remains open.
- [x] Preserve source mapping for each bounded await transition.

**Acceptance:** Strings, arrays, classes, and nested results remain valid across
multiple awaits and release on every exit path.

**Verification:** Run multi-await, forced-GC, cancellation, and failure fixtures
under sanitizers.

**Dependencies:** A5.

## A7. Async exception outcomes

**Objective:** Make async success, exception, and cancellation observable and
composable by callers.
**Implementation status:** The bounded one- and two-await codegen now maps a
cancelled child to a cancelled parent and copies a failed child error payload
and numeric source ID into an independently owned parent error slot. The frame
also carries bounded source-span start/end offsets through that propagation.
Compiler file/line mapping and nested exception chains remain open. The
bounded ABI defines a cancellation handler: cleanup runs first, then a handler
may publish a failure payload/span; otherwise cancellation remains terminal.

**Checklist:**

- [x] Define successful values and failure payload representation for the
      bounded frame ABI.
- [x] Preserve bounded source-span start/end metadata through one/two-await
      child-error propagation; compiler file/line mapping remains open.
- [x] Run cleanup before publishing an outcome in the bounded executor ABI.
- [x] Define bounded cancellation-handler failure semantics; compiler-level
      exception unwinding during cancellation remains open.

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
source file/line spans or nested exception chains.

`runtime/tests/task_dependency.c` additionally verifies that a failed child
wakes its parent, copies an independent error payload/source ID, and that
child cancellation remains distinguishable from failure. The codegen
single/two/three-await tests verify distinct resume states and both terminal
edges for every generated child.

**Status note:** The bounded successful-value/failure-payload,
cancelled-child propagation, and cleanup-before-outcome items are complete.
Compiler file/line mapping and nested exception unwinding remain open until
typed compiler outcome lowering exists. The bounded cancellation handler is
covered by `runtime/tests/task_outcomes.c`: owned cleanup completes before the
handler publishes a failure payload and source span.

**Dependencies:** A6.

## A8. ABI sanitizer suite

**Objective:** Prove frame and value ownership under hostile lifecycle conditions.

**Checklist:**

- [x] Exercise pending frames during forced GC.
- [x] Exercise cancellation, repeated polling, dropped handles, and failures.
- [x] Run memory and undefined-behavior sanitizers where supported.
- [x] Record reproducible seeds and minimized failures.

**Bounded evidence (2026-07-22):**
`runtime/tests/task_frame_sanitizer.c` covers a pending frame retaining a GC
capture across `aura_gc_collect`, repeated direct polling, executor
cancellation, a dropped pending handle cleaned by executor shutdown, and a
failed frame whose error payload is observed and released. This is a C ABI
fixture for the existing single-threaded frame/executor APIs; it does not claim
full async state-machine lowering or delayed wakeup support. The fixture is
run with ASAN/UBSAN and, when supported by the host toolchain, LSAN.

Reproducibility metadata is recorded in
`runtime/tests/sanitizer-seeds.tsv`. Each row uses seed `0` because the C ABI
fixtures are deterministic, names the minimized fixture source, and records the
sanitized compile command. `scripts/validate-sanitizer-seeds.sh` validates the
manifest and is run by `scripts/sanitizer-smoke.sh`; its negative duplicate-row
coverage is in `scripts/tests/validate-sanitizer-seeds.sh`.

**Acceptance:** No use-after-free, double-drop, invalid root, or leaked frame is
present in the mandatory suite.

**Verification:** Run the ABI stage on every supported native host.

**Dependencies:** A1–A7.
