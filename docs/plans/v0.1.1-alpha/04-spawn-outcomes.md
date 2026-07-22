# Workstream 04: Spawn Captures and Task Outcomes

Owner: Runtime + Compiler. Scope: 6 tasks.

## S1. Spawn frame creation

**Objective:** Execute non-empty spawned bodies as first-class task frames.
**Implementation status:** Partial lowering now covers empty and bounded
capture-free non-empty bodies made of effect-only calls with literal arguments
or an explicit unit return. Every frame receives a monotonic task identity and
initial state; unsupported capture/await/live-local bodies remain coupled to
A4–A6 and retain the explicit diagnostic/abort path.

**Checklist:**

- [x] Create an owned frame with stable task identity and initial state for the
      shipped empty and bounded capture-free non-empty subset.
- [x] Schedule the supported body exactly once under the deterministic
      executor; captures, awaits, and live locals remain unsupported.
- [x] Define immediate completion and abandoned-task behavior.
- [x] Expose spawn and terminal lifecycle events for diagnostics and race
      instrumentation.
- [x] Lower the proven capture-free effect-only subset to a real one-shot poll
      frame and verify native execution plus join completion.

**Acceptance:** A spawned body runs once and reaches a terminal state.

**Verification:** Run empty and bounded effect-only non-empty cases natively;
nested, capture, await, and repeatedly scheduled lowering remain unverified.

**Dependencies:** A1–A7.

## S2. Capture ownership

**Objective:** Keep every captured value valid until task completion.

**Checklist:**

- [ ] Copy or transfer captures according to ownership rules.
- [ ] Support Int, String, class, Array, and Fun captures.
- [x] Register, mark, release, and destroy captures with the frame. The
      bounded runtime slice roots owned capture storage, releases the root on
      replacement or frame destruction, and invokes its destroy callback once.
- [x] Reject unsupported borrowed captures before execution. The runtime
      setter rejects `AURA_TASK_BORROWED` without replacing a valid capture.

**Acceptance:** Captures survive await and are released exactly once.

**Verification:** Run capture, mutation, forced-GC, cancellation, and churn cases
under sanitizers.

**Dependencies:** S1, A3.

## S3. Join success

**Objective:** Return successful task results through a typed join handle.

**Checklist:**

- [x] Allow repeated join observation without resubmitting the executor-owned
      frame; handle/frame release after observation remains open.
- [x] Observe an executor-owned frame through the currently available ready
      queue until terminal; a genuinely pending frame remains unsupported by
      this bounded helper.
- [x] Retain the result in executor-owned frame storage and expose a borrowed
      observation snapshot; no transfer occurs during join.
- [ ] Release frame and handle safely after observation.

**Acceptance:** Immediate and delayed successful tasks produce identical results.

**Verification:** Test join-before-completion, join-after-completion, repeated
join, and dropped-handle cases.

**Dependencies:** S1, S2.

## S4. Join failure

**Objective:** Propagate task exceptions without losing source context or leaking
resources.

**Checklist:**

- [ ] Store failure payload and source location in the terminal outcome.
- [x] Make join distinguish failure from cancellation through terminal poll
      states and borrowed result/error snapshots.
- [ ] Clean captures, frames, and result storage on failure.
- [x] Define repeated observation of a failed task as a stable terminal result;
      failure payload/source retention and cleanup remain open.

**Acceptance:** A failed task is observable deterministically with no ownership
violation.

**Verification:** Run thrown-error, nested-error, forced-GC, and repeated-join
cases.

**Dependencies:** S3, A7.

## S5. Cancellation

**Objective:** Cancel ready and suspended tasks with defined cleanup and outcome.

**Implementation status:** Partial bounded runtime coverage now proves that an
owned capture attached to an executor task is released exactly once when a
task cancelled before its first poll or while pending is eventually destroyed
by the executor. The current API does not publish cleanup before the
`AURA_TASK_CANCELLED` state, and scheduler/await/I/O/handler boundary
semantics remain open.

**Checklist:**

- [ ] Define cancellation request, acknowledgement, and race with completion.
- [ ] Check cancellation at scheduler, await, I/O, and handler boundaries.
- [ ] Run cleanup exactly once before publishing cancellation.
- [x] Release an executor-owned capture exactly once for cancellation before
      first poll and while pending; the test observes cleanup during executor
      shutdown, after cancellation is published.
- [ ] Make join and unjoined-task behavior consistent after cancellation.

**Acceptance:** Cancellation cannot strand a frame, descriptor, capture, or
channel payload.

**Verification:** The runtime test covers cancellation before start and while
pending for capture cleanup. Cleanup-before-state-publication, during-resume,
after-completion, and descriptor/channel-payload cleanup remain unverified.

**Dependencies:** S3, S4, A7.

## S6. Unjoined failure policy

**Objective:** Implement the explicit policy for failures not observed by join.

**Checklist:**

- [ ] Choose and document logging, retention, propagation, or process-failure
      behavior according to the contract matrix.
- [ ] Preserve identity and source context for diagnosis.
- [ ] Prevent silent loss of failure payloads.
- [ ] Define shutdown and cancellation behavior.

**Acceptance:** Unjoined failure behavior is deterministic and release-tested.

**Verification:** Run isolated, nested, shutdown, and multiple-failure cases.

**Dependencies:** S4, S5, C3.
