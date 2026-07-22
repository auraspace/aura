# Workstream 04: Spawn Captures and Task Outcomes

Owner: Runtime + Compiler. Scope: 6 tasks.

## S1. Spawn frame creation

**Objective:** Execute non-empty spawned bodies as first-class task frames.
**Implementation status:** Foundation complete for the shipped empty-body spawn
slice. Every frame receives a monotonic task identity and initial state; the
deterministic executor schedules each submitted frame once. Non-empty body
lowering remains coupled to A4–A6 capture/await work.

**Checklist:**

- [x] Create an owned frame with stable task identity and initial state.
- [x] Schedule the body exactly once under the deterministic executor.
- [x] Define immediate completion and abandoned-task behavior.
- [x] Expose spawn and terminal lifecycle events for diagnostics and race
      instrumentation.

**Acceptance:** A spawned body runs once and reaches a terminal state.

**Verification:** Run empty, non-empty, nested, and repeatedly scheduled cases.

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

- [ ] Define handle ownership and single/multiple join behavior.
- [ ] Suspend the joiner until the task reaches a terminal state.
- [ ] Transfer or retain the result according to ABI rules.
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
- [ ] Make join distinguish failure from cancellation.
- [ ] Clean captures, frames, and result storage on failure.
- [ ] Define repeated observation of a failed task.

**Acceptance:** A failed task is observable deterministically with no ownership
violation.

**Verification:** Run thrown-error, nested-error, forced-GC, and repeated-join
cases.

**Dependencies:** S3, A7.

## S5. Cancellation

**Objective:** Cancel ready and suspended tasks with defined cleanup and outcome.

**Checklist:**

- [ ] Define cancellation request, acknowledgement, and race with completion.
- [ ] Check cancellation at scheduler, await, I/O, and handler boundaries.
- [ ] Run cleanup exactly once before publishing cancellation.
- [ ] Make join and unjoined-task behavior consistent after cancellation.

**Acceptance:** Cancellation cannot strand a frame, descriptor, capture, or
channel payload.

**Verification:** Test cancellation before start, while pending, during resume,
and after completion.

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
