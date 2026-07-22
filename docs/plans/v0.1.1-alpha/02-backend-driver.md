# Workstream 02: Backend Driver and C Backend

Owner: Compiler Expert. Scope: 5 tasks.

The alpha keeps C as the production backend, but isolates it behind a stable
backend-neutral compilation contract.

## B1. Compile options contract

**Objective:** Represent compilation choices independently from C emission.

**Checklist:**

- [x] Define backend, target, profile, feature, runtime ABI, output kind, and
      diagnostic mode as explicit options.
- [x] Preserve current defaults for existing projects.
- [x] Reject contradictory or incomplete combinations early.
- [x] Make options printable for diagnostics and artifact metadata.

**Acceptance:** Existing commands behave unchanged when no new option is set.

**Verification:** Run old CLI fixtures and invalid-option tests.

**Dependencies:** C1–C3.

## B2. Backend-neutral driver

**Objective:** Separate frontend/sema checking, backend lowering, linking, and
artifact reporting.

**Checklist:**

- [x] Make frontend/sema produce one checked representation per build.
- [x] Define backend input/output interfaces and error translation.
- [x] Keep source spans and structured diagnostics across the boundary.
- [x] Ensure test builds and ordinary builds use the same driver path.

**Acceptance:** The driver can select a backend without changing frontend
diagnostics or package loading semantics.

**Verification:** Compare diagnostics and exit statuses with the pre-driver
baseline across representative programs.

**Dependencies:** B1.

## B3. C backend adapter

**Objective:** Route current C generation and native linking through the new
driver without changing language behavior.

**Checklist:**

- [x] Adapt source generation to the backend interface.
- [x] Pass runtime, compiler, target, and profile settings explicitly.
- [x] Preserve test-runner and single-executable behavior.
- [x] Keep intermediate output available for debugging when requested.

**Acceptance:** The existing positive and negative corpus remains green through
the new path.

**Verification:** Run hello, packages, generics, exceptions, lambdas, GC,
async, and test-runner fixtures.

**Dependencies:** B2.

## B4. Target/backend validation

**Objective:** Fail early and clearly for unsupported compilation combinations.

**Checklist:**

- [x] Validate target availability, linker, runtime, profile, and backend.
- [x] Report supported alternatives and the failing capability.
- [x] Distinguish configuration errors from compiler errors.
- [x] Keep cross-compilation claims separate from native execution claims.

**Acceptance:** Unsupported combinations do not emit partial artifacts or invoke
an incorrect linker.

**Verification:** Test missing toolchains, unsupported targets, invalid profiles,
and incompatible runtime settings.

**Dependencies:** B2, C2.

## B5. Backend parity and metadata

**Objective:** Make C backend outputs and build decisions reproducible and
inspectable.

**Checklist:**

- [x] Record backend, target, profile, ABI, and feature identity in artifacts.
- [x] Preserve stable generated-output tests where output is contractual.
- [x] Make repeated builds produce equivalent behavior and metadata.
- [x] Define compatibility/debug backend behavior for future migrations.

**Acceptance:** A build can be audited to determine exactly which backend and
runtime contract produced it.

**Verification:** Run repeated builds, compare metadata, and execute the full
compatibility corpus.

**Dependencies:** B3, B4.
