# Workstream 08: Minimal Race Detector

Owner: Runtime + Compiler. Scope: 5 tasks.

## R1. Event and happens-before model

**Objective:** Define the minimum race semantics supported by alpha.
**Implementation status:** Foundation complete for executor and opt-in channel
events. Events carry task, address, source, kind, and monotonic sequence
identity; single-threaded executor order is the initial happens-before
relation. Concurrent refinement and suppression policy remain in the
instrumentation/reporting slices.
**Checklist:**

- [x] Define read/write identity and source location.
- [ ] Define spawn/join, await, cancellation, lock, and atomic edges.
- [x] Define opt-in channel send, receive, and close edges.
- [ ] Define deterministic ordering, suppression, and report identity.
      **Acceptance:** The model maps directly to the accepted concurrency contract.
      **Verification:** Review positive, synchronized, and intentionally racy traces.
      **Dependencies:** C1–C3, S1–S6.

## R2. Runtime event tracking

**Objective:** Record accesses and synchronization in opt-in development mode.
**Implementation status:** Foundation complete for the runtime tracker API,
executor lifecycle boundaries, and explicitly attached channels. The tracker
uses deterministic sequence numbers, growable storage, reset, stable indexed
inspection, and explicit opt-in executor/channel attachments; spawn, terminal
task, and channel send/receive/close events are recorded while uninstrumented
executors and channels remain silent.
**Checklist:**

- [x] Emit events at the executor spawn boundary.
- [x] Emit terminal completion, failure, and cancellation events.
- [x] Preserve task identity, logical time, and source mapping.
- [x] Emit opt-in channel send, receive, and close events with channel identity.
- [x] Keep tracking disabled in ordinary release mode; an executor without an
      attached tracker emits no events.
      **Acceptance:** Repeated deterministic runs produce the same event sequence.
      **Verification:** Run trace fixtures with channels, cancellation, and GC.
      **Dependencies:** R1, A1–A8.

## R3. Compiler instrumentation

**Objective:** Instrument compiler-generated accesses consistently.
**Checklist:**

- [ ] Instrument reads, writes, task boundaries, and synchronization operations.
- [ ] Preserve source spans through lowering and code generation.
- [ ] Ensure profile selection controls instrumentation.
      **Acceptance:** Generated operations are tracked without changing normal results.
      **Verification:** Compare instrumented and non-instrumented outputs.
      **Dependencies:** R1, R2, B2.

## R4. Stable race reports

**Objective:** Produce actionable and reproducible conflict reports.
**Checklist:**

- [ ] Report both conflicting accesses and their task/stack/source identities.
- [ ] Explain the missing synchronization edge.
- [ ] Define stable formatting and machine-readable output.
      **Acceptance:** A planted race identifies both sides without false ambiguity.
      **Verification:** Repeat positive, negative, and suppression fixtures.
      **Dependencies:** R2, R3.

## R5. CLI and regression suite

**Objective:** Expose the detector as a documented alpha workflow.
**Checklist:**

- [ ] Add the frozen race-test command/flag and exit behavior.
- [ ] Add planted-race, race-free, channel, cancellation, and GC fixtures.
- [ ] Verify release artifacts contain no detector state by default.
      **Acceptance:** Users can run one command and receive a stable pass/fail result.
      **Verification:** Run the race stage across development and release profiles.
      **Dependencies:** R4, P1–P2.
