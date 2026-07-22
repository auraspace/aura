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
- [x] Define spawn, join, and cancellation edges for tracked executor events;
      await, lock, and atomic edges remain open.
- [x] Define opt-in channel send, receive, and close edges.
- [x] Define deterministic ordering, suppression, and report identity.
      The bounded R4 report surface supplies stable sequence ordering,
      duplicate suppression, and a deterministic identity hash.
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
**Implementation status:** Development/test profiles now emit source-IDed
read/write hooks for local accesses and writes, attach source IDs to generated
task frames, and bracket join/await/channel synchronization calls. Release
profiles select the non-instrumented lowering. The runtime remains an event
collector only; conflict suppression and stable reports are R4.
**Checklist:**

- [x] Instrument reads, writes, task boundaries, and synchronization operations.
- [x] Preserve source spans through lowering and code generation.
- [x] Ensure profile selection controls instrumentation.
      **Acceptance:** Generated operations are tracked without changing normal results.
      **Verification:** Compare instrumented and non-instrumented outputs.
      **Dependencies:** R1, R2, B2.

## R4. Stable race reports

**Objective:** Produce actionable and reproducible conflict reports.
**Implementation status:** Complete for the bounded single-threaded alpha
report surface. The runtime identifies conflicting task/source/stack accesses,
suppresses duplicate identities, recognizes join/lock/channel hand-offs, and
renders stable human and JSON output. Concurrent vector-clock refinement and
the user-facing command remain R5/future scope.
**Checklist:**

- [x] Report both conflicting accesses and their task/stack/source identities.
- [x] Explain the missing synchronization edge.
- [x] Define stable formatting and machine-readable output.
      **Acceptance:** A planted race identifies both sides without false ambiguity.
      **Verification:** Repeat positive, negative, and suppression fixtures.
      **Dependencies:** R2, R3.

## R5. CLI and regression suite

**Objective:** Expose the detector as a documented alpha workflow.
**Implementation status:** Complete for the bounded CLI workflow. `aura race`
is a frozen alias for detector-enabled test execution with exit `0` for a
passing child, `1` for a failing child, and `2` for invalid CLI options. Human
and JSON output identify the race mode and detector state. The regression
script runs the planted-race report plus synchronized/race-free suppression
fixtures and checks that the default release-shaped C artifact does not
activate detector state.
**Checklist:**

- [x] Add the frozen race-test command/flag and exit behavior.
- [x] Add planted-race, race-free, channel, cancellation, and GC fixtures.
- [x] Verify release artifacts contain no detector state by default.
      **Acceptance:** Users can run one command and receive a stable pass/fail result.
      **Verification:** Run `scripts/race-regression.sh` across the detector
      development workflow and the default release-shaped `emit-c` artifact.
      **Dependencies:** R4, P1–P2.
