# S1 — Runtime & Compiler Hardening

**Status:** Complete

**Completed:** 2026-07-21

## Objective

Move the compiler/runtime from alpha dogfood quality toward a stronger release gate by fixing known ownership issues, enforcing strict linting, expanding regression coverage, and validating memory safety.

## Scope

- Includes runtime ownership, compiler diagnostics/quality, regression corpus, and CI gates.
- Excludes HTTPS registry support, publishing, the Windows release matrix, and the LLVM backend.

## Task list

### S1.1 — Fix `std.io.args()` ownership

**Description:** Ensure that `Array<String>` elements created from process arguments have valid ownership when the Array is dropped.

**Acceptance criteria:**

- [ ] Choose and document one strategy: duplicate argv elements with `strdup`, or represent the Array as non-owning.
- [ ] `std.io.args()` never frees memory it does not own.
- [ ] `examples/wc` runs successfully and exits with code 0.
- [ ] Add a regression test for `args()` and end-of-process teardown.
- [ ] Update the corresponding entry in `agents/debts.md`.

**Verification:**

- [ ] `cargo test --workspace`
- [ ] `bash scripts/check-corpus.sh`
- [ ] `aura run examples/wc -- examples/wc/README.md`
- [ ] Run the smoke test under ASan when supported by the toolchain.

**Dependencies:** None

**Likely files:** `runtime/aura_rt.c`, `std/io/src/lib.aura`, `crates/aura-codegen/`, `crates/aura-cli/`, `agents/debts.md`

### S1.2 — Make Clippy a required quality gate

**Description:** Fix all current warnings so the workspace passes with `-D warnings`, then add the command to CI.

**Acceptance criteria:**

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes on stable Rust.
- [ ] Do not use broad allows to hide unrelated warnings.
- [ ] CI runs Clippy together with the unit tests.

**Verification:**

- [ ] Run the full Clippy command locally.
- [ ] Confirm that the CI job passes on a pull request.

**Dependencies:** None

**Likely files:** `crates/aura-diagnostics/`, `crates/aura-parser/`, `crates/aura-sema/`, `crates/aura-codegen/`, `.github/workflows/ci.yml`

### S1.3 — Add runtime memory-safety regression coverage

**Description:** Create focused tests for ownership-sensitive paths: GC, Arrays, String elements, exceptions, and lambda environments.

**Acceptance criteria:**

- [ ] Add tests for GC mark/sweep with nested objects.
- [ ] Add tests for Array move, reassign, clone, clear, nested Arrays, and `Array<String>` drop.
- [ ] Add tests for lambda capture/free, nested Fun, and shared `var` boxes.
- [ ] Add tests for exception payload cleanup.
- [ ] Any discovered failure has a small reproducer in `corpus/`.

**Verification:**

- [ ] `cargo test --workspace`
- [ ] `bash scripts/check-corpus.sh`
- [ ] Run new corpus cases through `aura run` and `aura test`.

**Dependencies:** S1.1 should be completed before adding argv-based `Array<String>` tests.

**Likely files:** `corpus/`, `crates/aura-sema/src/tests.rs`, `crates/aura-cli/src/package/tests.rs`, `runtime/aura_rt.c`

### S1.4 — Run sanitizer smoke tests

**Description:** Add a way to run a focused corpus subset under AddressSanitizer and UndefinedBehaviorSanitizer to catch C runtime and codegen defects.

**Acceptance criteria:**

- [ ] Add a script or CI step that runs sanitizer tests on Linux.
- [ ] Cover at least hello, Array ownership, GC, exceptions, lambdas, and `examples/wc`.
- [ ] Sanitizers report no known leaks, invalid frees, use-after-free, or undefined behavior.
- [ ] Failure output retains enough context for debugging.

**Verification:**

- [ ] Run sanitizer tests locally on Linux or an equivalent toolchain.
- [ ] Sanitizer CI job passes, or any platform exception is documented.

**Dependencies:** S1.1, S1.3

**Likely files:** `scripts/`, `.github/workflows/ci.yml`, `runtime/`, `corpus/`

### S1.5 — Expand the compiler regression matrix

**Description:** Ensure that features completed in C0–C13 are tested through both parse/typecheck and native execution on critical paths.

**Acceptance criteria:**

- [ ] Green corpus cases are checked automatically without relying on manual output review.
- [ ] Add smoke runs for generics, interfaces, nullable flow, enum/match, exceptions, package imports, collections, and lambdas.
- [ ] Add smoke tests for `aura build`, `aura run`, `aura check`, and `aura test`.
- [ ] Expected-failure diagnostics in `corpus/diag` continue to be tested separately.

**Verification:**

- [ ] `bash scripts/check-corpus.sh`
- [ ] Corresponding smoke commands run in CI.
- [ ] Confirm that negative tests still fail with the expected diagnostic and exit code.

**Dependencies:** S1.3

**Likely files:** `scripts/check-corpus.sh`, `.github/workflows/ci.yml`, `corpus/`, `crates/aura-cli/`

### S1.6 — Audit CLI error paths and unsafe assumptions

**Description:** Review production paths in the CLI/compiler to reduce unnecessary panics and ensure consistent exit codes and diagnostics for user errors.

**Acceptance criteria:**

- [ ] Audit all `unwrap()`/`expect()`/`panic!()` calls in the CLI, loader, package manager, and codegen.
- [ ] Replace panics that can be triggered by user input with contextual errors.
- [ ] `check`, `build`, `run`, and `test` return stable exit codes for invalid input.
- [ ] Add tests for malformed manifests, missing dependencies, compiler errors, and runtime failures.

**Verification:**

- [ ] `cargo test --workspace`
- [ ] Run diagnostic corpus cases and CLI negative tests.
- [ ] Confirm that tested invalid inputs do not panic.

**Dependencies:** S1.5

**Likely files:** `crates/aura-cli/`, `crates/aura-diagnostics/`, `corpus/diag/`

### S1.7 — Close the S1 quality gate

**Description:** Consolidate all checks into a quality gate that can run before merge and release.

**Acceptance criteria:**

- [ ] Unit and integration tests pass.
- [ ] Clippy with `-D warnings` passes.
- [ ] Corpus checks and native smoke tests pass.
- [ ] Sanitizer smoke tests pass on the supported CI target.
- [ ] No known crash blocker remains undocumented in `agents/debts.md`.
- [ ] Add a changelog/plan note summarizing S1 changes.

**Verification:**

- [ ] Run the complete quality gate from a clean checkout.
- [ ] Pull request CI is green.
- [ ] Review the checklist before moving to S2.

**Dependencies:** S1.1–S1.6

**Likely files:** `.github/workflows/ci.yml`, `CHANGELOG.md`, `docs/plans/2026-07-21-s1-runtime-compiler-hardening.md`

## Recommended order

1. S1.1 — Fix `std.io.args()` ownership
2. S1.2 — Make Clippy a required quality gate
3. S1.3 — Add runtime memory-safety regression coverage
4. Checkpoint: tests, corpus, and Clippy pass
5. S1.4 — Run sanitizer smoke tests
6. S1.5 — Expand the compiler regression matrix
7. S1.6 — Audit CLI error paths
8. S1.7 — Close the S1 quality gate

## Exit criteria

- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `bash scripts/check-corpus.sh` passes.
- Runtime sanitizer smoke tests pass on Linux CI.
- `examples/wc` does not crash when using process arguments.
- CI has sufficient gates to prevent ownership/compiler regressions before S2.

## Completion notes

- `cargo test --workspace` passes with an isolated writable `XDG_CACHE_HOME`.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `bash scripts/check-corpus.sh` passes.
- `bash scripts/compiler-regression.sh` passes all 24 checks.
- `bash scripts/sanitizer-smoke.sh` passes all 6 sanitizer cases locally.
- Environment-sensitive registry cache tests are serialized with the existing registry test lock.
