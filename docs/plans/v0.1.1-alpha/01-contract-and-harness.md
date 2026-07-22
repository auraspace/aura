# Workstream 01: Contract and Test Harness

Owner: Docs/RFC + Test/Corpus. Scope: 6 tasks.

This workstream makes the alpha promise measurable before implementation work
expands. The matrix is the source of truth for what is required, supported,
deferred, or rejected.

## C1. Alpha contract matrix

**Objective:** Convert every mandatory alpha capability into a testable
requirement with an owner and a release status.

**Checklist:**

- [ ] Enumerate compiler, runtime, async, I/O, HTTP, build, package, FFI,
      diagnostics, and release requirements.
- [ ] Link each requirement to the relevant accepted design decision without
      copying implementation assumptions into the contract.
- [ ] Define `implemented`, `partial`, `blocked`, `deferred`, and `out of
    scope` consistently.
- [ ] Assign one workstream owner and one acceptance fixture to every gate.
- [ ] Record which claims require native execution and which are compile-only.

**Acceptance:** No mandatory requirement is missing an owner, test, status, or
release claim.

**Verification:** Review the matrix against the RFC set and run the current
baseline suite with the matrix attached to the result.

**Dependencies:** None.

## C2. Target and release policy

**Objective:** Freeze the supported host/target matrix for alpha.

**Checklist:**

- [ ] Confirm Linux amd64 and macOS amd64/arm64 support claims.
- [ ] Specify compiler, linker, runtime, system-library, and permission
      requirements for each target.
- [ ] Separate native-runtime-tested, cross-compiled, and unsupported targets.
- [ ] Define archive naming, checksum, signature, and installation guarantees.
- [ ] Define how a target is removed from support when acceptance fails.

**Acceptance:** A clean host can determine whether a target is supported before
compilation begins.

**Verification:** Run target preflight checks on each supported host and one
known unsupported target.

**Dependencies:** C1.

## C3. CLI, registry, and FFI contract

**Objective:** Freeze user-visible behavior for commands and external
integration boundaries.

**Checklist:**

- [ ] Specify commands, flags, exit codes, structured output, and error classes.
- [ ] Specify registry authentication, upload, download, checksum, and retry
      behavior.
- [ ] Specify self-update failure and rollback behavior.
- [ ] Specify supported FFI types, ownership, callbacks, and ABI errors.
- [ ] Specify HTTP server commands/examples and explicit non-goals.

**Acceptance:** CLI and integration tests can be written without inventing
behavior during implementation.

**Verification:** Add contract fixtures for success, invalid input, network
failure, checksum failure, and ABI mismatch.

**Dependencies:** C1.

## C4. Stage-based readiness harness

**Objective:** Provide one deterministic entry point for all alpha test stages.

**Checklist:**

- [ ] Add stages for frontend, backend, runtime, async, I/O, HTTP, build,
      registry, FFI, sanitizer, and release acceptance.
- [ ] Label every failure with stage, target, profile, and fixture identity.
- [ ] Support offline stages separately from network-required stages.
- [ ] Return stable aggregate exit codes while preserving individual failures.
- [ ] Allow a single stage or fixture to be rerun locally.

**Acceptance:** A failed run identifies the failing contract area without manual
log archaeology.

**Verification:** Run the full harness on a clean checkout and inject one
failure in each reporting layer.

**Dependencies:** C1.

## C5. Golden corpus split

**Objective:** Keep output-sensitive behavior stable while allowing internal
implementation changes.

**Checklist:**

- [ ] Separate syntax, diagnostics, type checking, generated output, runtime,
      async, I/O, HTTP, build, registry, and FFI fixtures.
- [ ] Add positive and negative cases for every mandatory contract.
- [ ] Record expected failures explicitly with a reason and owner.
- [ ] Make golden updates reviewable and deterministic.

**Acceptance:** Legacy behavior remains covered and new failures identify the
affected subsystem.

**Verification:** Run the corpus by stage and compare repeated outputs.

**Dependencies:** C1, C4.

## C6. Clean-host baseline

**Objective:** Establish a trustworthy baseline for release decisions.

**Checklist:**

- [ ] Document required tools, network access, permissions, and environment
      variables.
- [ ] Re-run the existing suite outside restricted cache/network conditions.
- [ ] Classify each failure as product, environment, flaky, or expected.
- [ ] Store command lines, target, compiler version, and result metadata.

**Acceptance:** The team can distinguish a real regression from a host setup
failure.

**Verification:** Repeat the baseline from a clean checkout on each supported
host class.

**Dependencies:** C1, C4.
