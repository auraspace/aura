# Workstream 01: Contract and Test Harness

Owner: Docs/RFC + Test/Corpus. Scope: 6 tasks.

This workstream makes the alpha promise measurable before implementation work
expands. The matrix is the source of truth for what is required, supported,
deferred, or rejected.

## Implementation status (2026-07-22)

- **C1:** Complete — 29-row contract matrix and validator are shipped in
  `contract-matrix.tsv` and `scripts/validate-alpha-contract.sh`.
- **C2:** Complete — target/release policy and CI/release enforcement cover
  Linux amd64, macOS arm64, and macOS amd64.
- **C3:** Partial — current CLI, offline registry, JSON test report, and
  deferred rich FFI/HTTP claims are recorded; publish/update and rich FFI
  implementation remain owned by workstreams 09–11.
- **C4:** Complete — `scripts/alpha-harness.sh` provides stage, fixture,
  target, profile, offline/network, rerun, and versioned JSON report support.
- **C5:** Partial — alpha corpus layout and golden policy are documented;
  capability-specific fixtures will land with their owning workstreams.
- **C6:** Complete — clean-host procedure is documented and the canonical
  release acceptance gate is wired into CI/release workflows.

## C1. Alpha contract matrix

**Objective:** Convert every mandatory alpha capability into a testable
requirement with an owner and a release status.

**Checklist:**

- [x] Enumerate compiler, runtime, async, I/O, HTTP, build, package, FFI,
      diagnostics, and release requirements.
- [x] Link each requirement to the relevant accepted design decision without
      copying implementation assumptions into the contract.
- [x] Define `implemented`, `partial`, `blocked`, `deferred`, and `out of
  scope` consistently.
- [x] Assign one workstream owner and one acceptance fixture to every gate.
- [x] Record which claims require native execution and which are compile-only.

**Acceptance:** No mandatory requirement is missing an owner, test, status, or
release claim.

**Verification:** Review the matrix against the RFC set and run the current
baseline suite with the matrix attached to the result.

**Dependencies:** None.

## C2. Target and release policy

**Objective:** Freeze the supported host/target matrix for alpha.

**Checklist:**

- [x] Confirm Linux amd64 and macOS amd64/arm64 support claims.
- [x] Specify compiler, linker, runtime, system-library, and permission
      requirements for each target.
- [x] Separate native-runtime-tested, cross-compiled, and unsupported targets.
- [x] Define archive naming, checksum, signature, and installation guarantees.
- [x] Define how a target is removed from support when acceptance fails.

**Acceptance:** A clean host can determine whether a target is supported before
compilation begins.

**Verification:** Run target preflight checks on each supported host and one
known unsupported target.

**Dependencies:** C1.

## C3. CLI, registry, and FFI contract

**Objective:** Freeze user-visible behavior for commands and external
integration boundaries.

**Contract:** Public registry reads use HTTPS and immutable version metadata;
private reads and publish use `GITHUB_TOKEN`/`gh` credentials. Every downloaded
archive is verified against its declared SHA-256 before extraction or cache
publication, and transient transport failures retry with bounded backoff.
Authentication, HTTP status, transport, checksum, and manifest failures have
distinct stable error classes. `aura publish --dry-run` performs all manifest,
version, contents, and dependency checks without network mutation; a real
publish uploads the archive and index metadata only after those checks pass.

Self-update downloads to isolated temporary storage, verifies checksum and
signature before activation, atomically replaces the active version, and keeps
the previous version as rollback state. Any interrupted download, failed
verification, or failed activation leaves the previous version active.

The supported FFI contract is limited to target-guarded C declarations with
`Int`, `Bool`, `String` handle, and `Unit` values. Strings are borrowed for the
duration of a call unless an explicit copy/transfer operation is requested;
foreign pointers are opaque, nullable, pinned values and must be explicitly
released. Callbacks retain their environment until deregistration and cannot
cross task/await boundaries in the alpha contract. ABI mismatches are hard
errors before linking.

The HTTP surface is explicitly deferred from the alpha executable claim. The
reserved future command is `aura http serve <package-or-example>`; no HTTP
server, socket API, routing behavior, or implicit network permission is
available until workstream 11 supplies its parser, limits, lifecycle, and
native acceptance fixtures.

These rules make success, invalid input, network failure, checksum failure,
rollback failure, and ABI mismatch independently testable without inventing
behavior during implementation.

**Checklist:**

- [x] Specify commands, flags, exit codes, structured output, and error classes.
- [x] Specify registry authentication, upload, download, checksum, and retry
      behavior.
- [x] Specify self-update failure and rollback behavior.
- [x] Specify supported FFI types, ownership, callbacks, and ABI errors.
- [x] Specify HTTP server commands/examples and explicit non-goals.

**Acceptance:** CLI and integration tests can be written without inventing
behavior during implementation.

**Verification:** Add contract fixtures for success, invalid input, network
failure, checksum failure, and ABI mismatch.

**Dependencies:** C1.

## C4. Stage-based readiness harness

**Objective:** Provide one deterministic entry point for all alpha test stages.

**Checklist:**

- [x] Add stages for frontend, backend, runtime, async, I/O, HTTP, build,
      registry, FFI, sanitizer, and release acceptance.
- [x] Label every failure with stage, target, profile, and fixture identity.
- [x] Support offline stages separately from network-required stages.
- [x] Return stable aggregate exit codes while preserving individual failures.
- [x] Allow a single stage or fixture to be rerun locally.

**Acceptance:** A failed run identifies the failing contract area without manual
log archaeology.

**Verification:** Run the full harness on a clean checkout and inject one
failure in each reporting layer.

**Dependencies:** C1.

## C5. Golden corpus split

**Objective:** Keep output-sensitive behavior stable while allowing internal
implementation changes.

**Checklist:**

- [x] Separate syntax, diagnostics, type checking, generated output, runtime,
      async, I/O, HTTP, build, registry, and FFI fixtures.
- [ ] Add positive and negative cases for every mandatory contract.
- [x] Record expected failures explicitly with a reason and owner.
- [x] Make golden updates reviewable and deterministic.

**Acceptance:** Legacy behavior remains covered and new failures identify the
affected subsystem.

**Verification:** Run the corpus by stage and compare repeated outputs.

**Dependencies:** C1, C4.

## C6. Clean-host baseline

**Objective:** Establish a trustworthy baseline for release decisions.

**Checklist:**

- [x] Document required tools, network access, permissions, and environment
      variables.
- [x] Re-run the existing suite outside restricted cache/network conditions.
- [x] Classify each failure as product, environment, flaky, or expected.
- [x] Store command lines, target, compiler version, and result metadata.

**Acceptance:** The team can distinguish a real regression from a host setup
failure.

**Verification:** Repeat the baseline from a clean checkout on each supported
host class.

**Dependencies:** C1, C4.
