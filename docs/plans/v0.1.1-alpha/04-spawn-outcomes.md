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

**Implementation status:** Complete for the bounded single-threaded executor
slice. A terminal frame can be released through its handle slot after
observation; release unlinks the frame before destruction and clears the slot,
so repeated release is a no-op. Suspended/non-terminal dropped handles remain
executor-owned until cancellation or shutdown and are outside this S3 slice.

**Checklist:**

- [x] Allow repeated join observation without resubmitting the executor-owned
      frame; handle/frame release after observation remains open.
- [x] Observe an executor-owned frame through the currently available ready
      queue until terminal; a genuinely pending frame remains unsupported by
      this bounded helper.
- [x] Retain the result in executor-owned frame storage and expose a borrowed
      observation snapshot; no transfer occurs during join.
- [x] Release an executor-owned terminal frame and clear its handle through an
      idempotent API; unlinking is tested for head, middle, and tail nodes.

**Acceptance:** Immediate and delayed successful tasks produce identical results.

**Verification:** Test join-before-completion, join-after-completion, repeated
join, repeated release, dropped-handle, and sanitizer cases.

**Dependencies:** S1, S2.

## S4. Join failure

**Objective:** Propagate task exceptions without losing source context or leaking
resources.

**Implementation status:** Complete for the bounded single-threaded executor
slice. Failed frames retain a borrowed error snapshot and stable source ID
through repeated joins. Error/result release clears the ownership slot and
removes its GC root before invoking user cleanup, so terminal release and
frame destruction are idempotent with respect to repeated observation and
re-entrant cleanup inspection.

**Checklist:**

- [x] Store failure payload and bounded source identity in the terminal outcome.
- [x] Make join distinguish failure from cancellation through terminal poll
      states and borrowed result/error snapshots.
- [x] Release an owned capture and failure payload exactly once when a failed
      frame is destroyed; clear the payload slot before user cleanup.
- [x] Define repeated observation of a failed task as a stable terminal result;
      failure payload/source retention and cleanup are covered by focused tests.

**Acceptance:** A failed task is observable deterministically with no ownership
violation.

**Verification:** `runtime/tests/task_join.c` covers thrown-error, repeated
join, stable source identity, terminal release, and cleanup re-entrancy
inspection. Nested compiler error lowering and forced-GC end-to-end cases remain
deferred with the async state-machine work.

**Dependencies:** S3, A7.

## S5. Cancellation

**Objective:** Cancel ready and suspended tasks with defined cleanup and outcome.

**Implementation status:** The bounded single-threaded executor now defines
cancellation as a request/acknowledgement pair. `cancel()` accepts a request
only for a non-terminal executor-owned frame; the next scheduler poll
acknowledges it as `AURA_TASK_CANCELLED` after releasing pending work and
captures. If completion is published first, the terminal completion wins and
the request is rejected. Joined and unjoined cancelled frames use the same
terminal state and cleanup path; scheduler/await/I/O/handler boundary
semantics remain open.

**Checklist:**

- [x] Define cancellation request, acknowledgement, and race with completion
      for ready and pending frames: request acceptance is observable through
      `aura_task_frame_cancel_requested`, acknowledgement through the terminal
      state, and completion wins when published first.
- [ ] Check cancellation at scheduler, await, I/O, and handler boundaries.
- [x] Run pending-operation and capture cleanup exactly once before publishing
      `AURA_TASK_CANCELLED` in the bounded executor.
- [x] Release an executor-owned capture exactly once for cancellation before
      first poll and while pending; the test observes cleanup during executor
      shutdown, after cancellation is published.
- [x] Make join and unjoined-task behavior consistent after cancellation: both
      retain the same terminal state, exclude the failure hook, and release
      owned storage exactly once.

**Acceptance:** Cancellation cannot strand a frame, descriptor, capture, or
channel payload.

**Verification:** `runtime/tests/task_cancellation.c` covers request versus
acknowledgement, ready and pending frames, completion-before-cancel ordering,
joined and unjoined cancellation, repeated requests, and cleanup-before-state
publication under ASAN/UBSAN. During-resume, descriptor/channel-payload
cleanup, and cancellation at await/I/O/handler boundaries remain unverified.

**Dependencies:** S3, S4, A7.

## S6. Unjoined failure policy

**Objective:** Implement the explicit policy for failures not observed by join.

**Implementation status:** Bounded runtime policy complete. A failed terminal
frame released without a failed join, or reclaimed during executor shutdown,
invokes a borrowed diagnostic hook exactly once. The default hook logs task and
source identity plus error size to stderr; callers may install a deterministic
structured hook. Joined failures suppress the unjoined report, while
cancellation is never reported as failure.

**Checklist:**

- [x] Choose and document bounded logging/retention behavior: default stderr
      logging or a borrowed callback hook.
- [x] Preserve task identity and source context for diagnosis.
- [x] Prevent silent loss of unjoined failure payloads.
- [x] Define shutdown behavior and keep cancellation out of failure reports.

**Acceptance:** Unjoined failure behavior is deterministic and release-tested.

**Verification:** `runtime/tests/task_unjoined_failure.c` covers isolated
release, joined suppression, multiple shutdown failures, and cancellation.
Nested compiler-generated failure payloads remain deferred with A6/A7.

**Dependencies:** S4, S5, C3.
