# Implementation Plan: C22 Async, Tasks, and Channels

| Field  | Value                                                     |
| ------ | --------------------------------------------------------- |
| Opened | 2026-07-22                                                |
| Status | Closed with partial implementation; release work deferred |
| Scope  | Single-threaded async/task MVP; release work excluded     |

## Overview

C22 converts RFC-003's async/task direction into a deterministic, single-threaded MVP. C21 lexical `ref` values cannot cross `await`, task, or channel boundaries. OS-thread scheduling, concurrent GC, blocking I/O integration, and release work remain deferred.

Each task should land as one focused commit. Agents must use disjoint write sets and integrate in dependency order.

## Landed status (C22t, 2026-07-22)

| Task | Status                                                                     | Commit    |
| ---- | -------------------------------------------------------------------------- | --------- |
| C22a | Done — terminology and single-threaded MVP boundary frozen                 | `d8e4640` |
| C22b | Done — grammar, examples, invalid forms, and spans specified               | `9f01b8e` |
| C22c | Done — task/handle and bounded FIFO channel contracts specified            | `3a5f9d1` |
| C22d | Done — async borrow barriers and diagnostic wording specified              | `79c4a32` |
| C22e | Done — async AST/file/expression nodes and lexer keywords wired            | `4bd0127` |
| C22f | Done — async functions and `await` parse                                   | `fab198b` |
| C22g | Done — task/channel operations parse                                       | `1b48d64` |
| C22h | Done — async task/handle/channel validation in sema                        | `770f935` |
| C22i | Done — borrow barriers enforced in sema                                    | `cd645d5` |
| C22j | Done — task-frame ABI and ownership/destruction contract implemented       | `84d0e81` |
| C22k | Done — deterministic single-threaded executor implemented                  | `695d760` |
| C22l | Partial — no-await task-frame lowering only; await state machine deferred  | `5354487` |
| C22m | Partial — empty spawn/join/cancel integration; captures and await deferred | `185a3c7` |
| C22n | Done — bounded FIFO channels, close, wakeups, and cleanup                  | `1064aeb` |
| C22o | Done for supported payload slice — Int/String/class channel lowering       | `4d4929f` |
| C22p | Done — green and expected-fail async corpus fixtures                       | `b86fc7c` |
| C22q | Done — stable async diagnostics and JSON/pretty metadata                   | `4a6fa78` |
| C22r | Done — churn/leak coverage and sanitizer checks where supported            | `c919aef` |
| C22s | Done — ownership/GC audit; residual async root debt recorded               | `edd1350` |
| C22t | This status/documentation commit                                           | pending   |

### Verification recorded by C22t

- `cargo test -p aura-sema`: 67 passed.
- `cargo test -p aura-codegen`: 3 passed.
- Strict C11 runtime task/channel tests and ASan/UBSan checks passed; LeakSanitizer is unavailable on the current platform.
- Async green fixtures and focused diagnostics passed; full corpus/CLI/workspace gates still have known failures from unsupported lowering and sandbox/network/cache constraints.

## Tasks

### Task C22a: Freeze async/task terminology

**Description:** Synchronize RFC-003 vocabulary for async functions, tasks, handles, suspension, join, cancellation, and channels.

- [x] RFC has one vocabulary and explicit single-threaded/non-release boundaries.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** None
      **Write set:** `docs/rfc/RFC-003-memory-model-concurrency.md; docs/roadmap.md`
      **Estimated scope:** S

### Task C22b: Define async/task syntax

**Description:** Specify grammar and examples for `async fun`, `await`, `spawn`, `join`, and cancellation.

- [x] Valid/invalid forms and parser span behavior are specified.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22a
      **Write set:** `docs/rfc/; docs/plans/`
      **Estimated scope:** S

### Task C22c: Define task and channel types

**Description:** Freeze task-result/handle behavior, bounded channels, ordering, close, failure, and cancellation outcomes.

- [x] Ownership crossing task/channel boundaries and FIFO/capacity rules are explicit.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22a
      **Write set:** `docs/rfc/RFC-003-memory-model-concurrency.md; docs/rfc/RFC-007-standard-library.md`
      **Estimated scope:** S

### Task C22d: Specify borrow barriers

**Description:** Extend C21 rules to reject `ref T` across await, spawn, send, receive, or task-owned storage.

- [x] Diagnostics identify the boundary causing every borrow escape.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22b, C22c
      **Write set:** `docs/rfc/RFC-002-type-system.md; docs/rfc/RFC-003-memory-model-concurrency.md`
      **Estimated scope:** S

### Checkpoint 1: Contract review

- [ ] All preceding tasks are committed and targeted tests pass.
- [ ] No implementation lane starts with an unresolved contract.

### Task C22e: Add async/task AST nodes

**Description:** Represent async declarations, await, task creation, join, cancellation, and channel operations with spans.

- [x] Existing AST consumers remain exhaustive and compile.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22b
      **Write set:** `crates/aura-ast/**`
      **Estimated scope:** M

### Task C22f: Parse async functions and await

**Description:** Implement parser support for async declarations and await forms.

- [x] Valid forms parse and invalid placement has focused diagnostics.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22e
      **Write set:** `crates/aura-parser/**`
      **Estimated scope:** M

### Task C22g: Parse spawn, join, cancellation, and channels

**Description:** Add parser support for task/channel operations and generic channel element types.

- [x] Malformed operations report source spans.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22e, C22f
      **Write set:** `crates/aura-parser/**; corpus/async/**`
      **Estimated scope:** M

### Task C22h: Type-check async results and handles

**Description:** Add sema types and rules for async return values, task handles, join, and cancellation.

- [x] Join recovers result type; invalid operands are rejected structurally.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22d, C22f, C22g
      **Write set:** `crates/aura-sema/**`
      **Estimated scope:** M

### Task C22i: Enforce async borrow barriers

**Description:** Implement C22d in sema for await, spawned-task capture, and channel payloads.

- [x] Borrowed values are rejected; owned values remain valid.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22h
      **Write set:** `crates/aura-sema/**`
      **Estimated scope:** M

### Checkpoint 2: Front-end foundation

- [ ] All preceding tasks are committed and targeted tests pass.
- [ ] No implementation lane starts with an unresolved contract.

### Task C22j: Design task-frame runtime ABI

**Description:** Define additive C ABI for task frames, poll state, ready state, result storage, and destruction.

- [x] Ownership and cleanup rules are documented; strict C11 compile passes.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22a, C22h
      **Write set:** `runtime/**; docs/rfc/RFC-006-runtime.md`
      **Estimated scope:** M

### Task C22k: Implement single-threaded executor

**Description:** Add deterministic ready queue and poll loop for task frames.

- [x] Tasks yield/resume without blocking and shutdown releases frames.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22j
      **Write set:** `runtime/**; runtime/tests/**`
      **Estimated scope:** L

### Task C22l: Lower async functions to state machines

**Description:** Lower async bodies into explicit states at await points using task frames.

- [ ] No-await bodies compile and run through task frames; await state-machine lowering remains deferred.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22e, C22h, C22j
      **Write set:** `crates/aura-codegen/**`
      **Estimated scope:** L

### Task C22m: Lower spawn, join, and cancellation

**Description:** Connect task operations to executor and typed task handles.

- [ ] Empty spawn/join/cancel are wired; non-empty capture lowering and full failure propagation remain deferred.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22k, C22l
      **Write set:** `crates/aura-codegen/**; runtime/**; std/task/**`
      **Estimated scope:** L

### Task C22n: Implement bounded channel runtime

**Description:** Add FIFO bounded channels, close behavior, wait queues, wakeups, and queued-value destruction.

- [x] Capacity/order/close behavior matches C22c for the implemented runtime payload slice.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22c, C22k, C22j
      **Write set:** `runtime/**; runtime/tests/**`
      **Estimated scope:** L

### Task C22o: Lower typed channel send/receive

**Description:** Add codegen and stdlib glue for Int, String, and class payloads, rejecting borrowed payloads.

- [x] Closed outcomes are typed and ownership is balanced for Int, String, and class payloads.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22i, C22n
      **Write set:** `crates/aura-codegen/**; std/task/**; runtime/**`
      **Estimated scope:** L

### Checkpoint 3: Runtime vertical slice

- [ ] All preceding tasks are committed and targeted tests pass.
- [ ] No implementation lane starts with an unresolved contract.

### Task C22p: Add async/task corpus matrix

**Description:** Cover no-await, await, spawn/join, cancellation, channels, and invalid borrow-boundary cases.

- [x] Fixtures are deterministic and require no network or OS threads; unsupported lowering cases are expected-fail.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22m, C22o
      **Write set:** `corpus/async/**; corpus/README.md`
      **Estimated scope:** M

### Task C22q: Add structured async diagnostics

**Description:** Add stable codes/notes for await barriers, invalid task operations, cancellation, and channel state errors.

- [x] JSON and pretty output include operation and span.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22i, C22p
      **Write set:** `crates/aura-diagnostics/**; crates/aura-cli/**; docs/rfc/RFC-012-cli.md`
      **Estimated scope:** M

### Task C22r: Add task churn and leak checks

**Description:** Stress spawn/join/cancel and channel close paths for frame, queue, and payload leaks.

- [x] Cleanup is idempotent; sanitizer runs where supported.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22k, C22m, C22n, C22o, C22p
      **Write set:** `runtime/tests/**; corpus/async/**; scripts/**`
      **Estimated scope:** M

### Task C22s: Audit C21 ownership and GC interaction

**Description:** Verify task frames keep owned payloads alive and never retain C21 borrowed views across suspension.

- [x] Supported ownership paths were audited and residual async GC-root limitations were recorded.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22m, C22o, C22r
      **Write set:** `runtime/**; crates/aura-codegen/**; agents/debts.md`
      **Estimated scope:** M

### Task C22t: Close C22 and refresh status

**Description:** Synchronize plan, RFCs, roadmap, README, corpus guidance, and technical debt; keep release deferred.

- [x] C22 statuses/hashes and deferred next steps are recorded; release remains deferred.
      **Verification:** Add focused tests/checks for this task; preserve all existing regressions.
      **Dependencies:** C22a–s
      **Write set:** `docs/plans/**; docs/rfc/**; docs/roadmap.md; README.md; agents/debts.md`
      **Estimated scope:** M

## Conflict-free lanes

| Lane      | Tasks        | Write set                                      |
| --------- | ------------ | ---------------------------------------------- |
| Contracts | C22a–d       | `docs/rfc`, `docs/plans`                       |
| Frontend  | C22e–g       | `crates/aura-ast`, `crates/aura-parser`        |
| Sema      | C22h–i       | `crates/aura-sema`                             |
| Runtime   | C22j–k, C22n | `runtime`                                      |
| Lowering  | C22l–m, C22o | `crates/aura-codegen`                          |
| QA/docs   | C22p–t       | `corpus`, `scripts`, `docs`, `agents/debts.md` |

Shared contracts land first. Agents must not edit another lane's write set.

## Deferred beyond C22

- OS-thread scheduler and M:N parallel execution.
- Concurrent GC and cross-thread sharing primitives.
- Blocking-I/O integration with the async reactor.
- Production release rehearsal, registry publication, and signing changes.
- Advanced structured-concurrency policies beyond the MVP cancellation contract.

## Risks and mitigations

| Risk                                                  | Impact   | Mitigation                                                     |
| ----------------------------------------------------- | -------- | -------------------------------------------------------------- |
| State-machine lowering mishandles locals across await | High     | Start with no-await/one-await slices and inspect generated C.  |
| Borrowed values survive suspension                    | Critical | Enforce C22d before lowering and add negative corpus fixtures. |
| Task/channel ownership leaks                          | High     | Add explicit destruction paths and churn tests.                |
| Runtime ABI grows without contract                    | Medium   | Freeze C22j ABI and compile strict C11.                        |

## Definition of done

- [x] C22a–t are implemented or explicitly marked partial/deferred with reasons.
- [x] Supported single-threaded task, cancellation, and bounded-channel examples pass; await and non-empty spawn remain unsupported.
- [ ] Full C21 borrow/GC regression gate is not claimed complete; see verification limitations above and `agents/debts.md`.
- [x] Verification results and commit hashes are recorded.
- [x] Release remains deferred unless separately requested.
