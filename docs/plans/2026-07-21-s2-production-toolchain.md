# S2 — Production Toolchain & Release Readiness

**Status:** Implementation complete; release pending

## Objective

Move Aura from hardened alpha tooling toward a production-usable release for
supported Unix targets. S2 focuses on dependable package consumption,
reproducible release artifacts, installer safety, and an explicit support
contract.

## Supported targets

S2 release gates cover:

- Linux amd64 (`x86_64-unknown-linux-gnu`)
- macOS arm64 (`aarch64-apple-darwin`)
- macOS amd64 (`x86_64-apple-darwin`, cross-built on macOS arm64)

Windows amd64 is explicitly deferred and must not block S2 completion.

## Non-goals

- Windows amd64 CI or release artifacts
- LLVM backend
- Async/tasks/channels
- True borrow types or `Array` of interface elements
- Generic `HashMap<K, V>`
- Registry hosting, user account management, or an externally coordinated
  publish service unless its API is already available

## Architecture decisions

- Keep the local fixture registry for offline tests and add a separate HTTPS
  transport path for production use.
- Verify registry package bytes against the lockfile checksum before making
  them visible in the cache.
- Use atomic temporary files/directories for cache and release outputs so an
  interrupted operation cannot leave a misleading successful state.
- Treat the three Unix targets above as the supported S2 contract; document
  other targets as source-build or unsupported rather than implying coverage.

## Task list

### Phase 1: Package consumption

### Task S2.1: Implement verified HTTPS registry fetch

**Description:** Add production HTTPS downloads for registry metadata and crate
archives while preserving the existing offline fixture path used by tests and
CI.

**Acceptance criteria:**

- [x] HTTPS metadata and archive downloads are supported with clear timeout and
      non-success response errors.
- [x] Downloaded archives are verified against the lockfile checksum before
      extraction or cache publication.
- [x] Partial downloads cannot be mistaken for valid cached packages.
- [x] Offline fixture tests remain deterministic and do not require network
      access.

**Verification:**

- [x] Unit-test URL, HTTP error, checksum mismatch, and interrupted-download
      paths.
- [x] Run an HTTPS integration test against a local test server or checked-in
      transport fixture.
- [x] `cargo test --workspace`

**Dependencies:** None

**Files likely touched:** `crates/aura-cli/src/package/fetch.rs`,
`crates/aura-cli/src/package/registry.rs`, `crates/aura-cli/src/package/tests.rs`,
`crates/aura-cli/Cargo.toml`

**Estimated scope:** Medium

### Task S2.2: Resolve nested registry dependencies deterministically

**Description:** Extend locked registry resolution so a package's registry
dependencies are resolved, fetched, and materialized recursively without
changing the existing path-dependency behavior.

**Acceptance criteria:**

- [x] Nested registry dependencies are represented in and validated from
      `aura.lock`.
- [x] Resolution is deterministic and rejects cycles, missing versions, and
      checksum mismatches with actionable diagnostics.
- [x] Repeated builds reuse the verified cache and do not refetch unchanged
      packages.
- [x] Existing path and mixed path/registry package graphs continue to work.

**Verification:**

- [x] Add fixture coverage for one nested dependency, a cycle, and a missing
      locked package.
- [x] Build and run a package graph with both path and registry dependencies.
- [x] `bash scripts/compiler-regression.sh`

**Dependencies:** S2.1

**Files likely touched:** `crates/aura-cli/src/package/load.rs`,
`crates/aura-cli/src/package/lock.rs`, `crates/aura-cli/src/package/fetch.rs`,
`crates/aura-cli/src/package/tests.rs`

**Estimated scope:** Medium

### Checkpoint: Package consumption

- [x] Local fixture and HTTPS paths pass the same resolution assertions.
- [x] Nested dependencies build from a clean cache.
- [x] A failed or interrupted fetch leaves no usable corrupt cache entry.

### Phase 2: Distribution and installation

### Task S2.3: Harden release packaging and artifact verification

**Description:** Make release tarballs self-describing and reproducible for the
three supported targets, with validation for embedded runtime, std packages,
version metadata, and checksums.

**Acceptance criteria:**

- [x] Each supported target produces exactly one correctly named tarball and
      checksum file.
- [x] The packaged CLI embeds or locates the runtime and required std packages
      without the source repository.
- [x] Artifact verification fails on a modified archive or checksum.
- [x] Rebuilding from the same revision produces stable package contents except
      for explicitly documented toolchain metadata.

**Verification:**

- [x] `TAG_VERSION=0.2.0-alpha bash scripts/package-release.sh`
- [x] Verify each archive with `sha256sum --check` or the platform equivalent.
- [x] Run `bash scripts/install-smoke.sh --local-pkg` on a supported Unix host.
- [x] Inspect archive contents and run `aura version`, `aura new`, and
      `aura run` from outside the repository.

**Dependencies:** None

**Files likely touched:** `scripts/package-release.sh`,
`scripts/install-smoke.sh`, `.github/workflows/release.yml`,
`docs/releases/README.md`

**Estimated scope:** Medium

### Task S2.4: Make installer and version-manager failure-safe

**Description:** Audit `install.sh` and `avm` for interrupted downloads, invalid
versions, checksum failures, PATH setup, and switching between installed
versions.

**Acceptance criteria:**

- [x] Installer validates the selected platform and checksum before activation.
- [x] Failed installs do not replace the current version or leave a partial
      active directory.
- [x] `avm` list/use/remove behavior is deterministic for missing and malformed
      installations.
- [x] Install documentation matches actual shell, directory, and PATH behavior.

**Verification:**

- [x] Exercise success, checksum mismatch, interrupted download, unsupported
      platform, and version-switch scenarios in a temporary `AURA_HOME`.
- [x] `bash -n scripts/install.sh scripts/avm scripts/install-smoke.sh`
- [x] `bash scripts/install-smoke.sh --local-pkg`

**Dependencies:** S2.3

**Files likely touched:** `scripts/install.sh`, `scripts/avm`,
`scripts/install-smoke.sh`, `docs/guide/install.md`

**Estimated scope:** Medium

### Task S2.5: Define and enforce the supported platform contract

**Description:** Align CI, release workflow, installer detection, and user
documentation around the three S2 targets while explicitly excluding Windows
amd64.

**Acceptance criteria:**

- [x] CI runs the S2 quality and packaging checks for Linux amd64 and macOS
      arm64/amd64.
- [x] Release artifacts and installer names use one documented OS/architecture
      convention.
- [x] Unsupported targets produce an actionable message with the source-build
      alternative.
- [x] Windows amd64 is labeled deferred everywhere it is mentioned and is not
      part of the required gate.

**Verification:**

- [x] Validate the matrix and artifact names in CI configuration.
- [x] Run installer platform-detection tests with representative `uname`/
      architecture inputs.
- [x] Review `README.md`, install guide, release notes, and roadmap for
      consistent support claims.

**Dependencies:** S2.3, S2.4

**Files likely touched:** `.github/workflows/ci.yml`,
`.github/workflows/release.yml`, `scripts/install.sh`, `README.md`,
`docs/guide/install.md`, `docs/releases/README.md`

**Estimated scope:** Medium

### Checkpoint: Distribution

- [x] A clean supported host can install a packaged release and run a generated
      hello project without the repository.
- [x] Every published artifact has a matching checksum and verified smoke test.
- [x] Installer and release documentation agree with CI behavior.

### Phase 3: Ship gate

### Task S2.6: Add production release acceptance tests

**Description:** Consolidate package, install, CLI, compiler, and runtime checks
into a release acceptance gate that can run before tagging a production release.

**Acceptance criteria:**

- [x] The gate covers clean-cache registry consumption, package build/run/test,
      artifact verification, install, and version switching.
- [x] Failures identify target, artifact, and reproduction command.
- [x] The gate is runnable locally without GitHub credentials.
- [x] Network-dependent checks are isolated from the offline PR gate.

**Verification:**

- [x] Run the complete gate from a clean checkout and temporary cache/home.
- [x] Confirm intentional failures for checksum, malformed manifest, and missing
      registry package are reported without panics.
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `bash scripts/check-corpus.sh`
- [x] `bash scripts/compiler-regression.sh`
- [x] `bash scripts/sanitizer-smoke.sh`

**Dependencies:** S2.1–S2.5

**Files likely touched:** `scripts/`, `.github/workflows/ci.yml`,
`.github/workflows/release.yml`, `docs/releases/README.md`

**Estimated scope:** Medium

### Task S2.7: Close the S2 release contract

**Description:** Record the final S2 support boundary, remaining technical debt,
release checklist, and candidate version notes so maintainers can make a
production ship decision from one source of truth.

**Acceptance criteria:**

- [x] `agents/debts.md` contains only known, actionable deferred items.
- [x] `docs/roadmap.md`, README, install guide, and release notes use consistent
      status and platform claims.
- [x] A release checklist names all required artifacts, checks, and rollback
      actions.
- [x] Windows amd64 remains explicitly deferred and is not listed as a S2 exit
      criterion.

**Verification:**

- [x] Review the release checklist against a dry-run package and install.
- [x] Run the full S2 acceptance gate from S2.6.
- [x] `git diff --check`

**Dependencies:** S2.6

**Files likely touched:** `agents/debts.md`, `docs/roadmap.md`, `README.md`,
`docs/guide/install.md`, `docs/releases/`, `CHANGELOG.md`

**Estimated scope:** Small

## Recommended order and parallelization

1. Start S2.1 and S2.3 in parallel; they have disjoint code paths.
2. Start S2.2 after S2.1; it owns dependency graph and lock behavior.
3. Start S2.4 after S2.3; it owns installer/version-manager behavior.
4. Start S2.5 after the release artifact contract in S2.3 and installer work in
   S2.4; keep workflow/document edits coordinated.
5. Run S2.6, then S2.7 sequentially as the final gate and documentation close. ✓

## Risks and mitigations

| Risk                                                             | Impact | Mitigation                                                                                            |
| ---------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------- |
| HTTPS client behavior differs across platforms                   | High   | Use a narrow transport abstraction and test against a local HTTPS fixture before CI integration.      |
| Registry cache corruption after interruption                     | High   | Write to temporary paths, verify checksums, then atomically rename.                                   |
| Cross-built macOS amd64 artifact is not executable on the runner | Medium | Validate Mach-O type and checksum; run the artifact on a native amd64 host when available.            |
| Installer changes break existing users                           | High   | Preserve current layout, use temporary activation, and test upgrade/rollback in isolated `AURA_HOME`. |
| Scope expands into publish or Windows support                    | Medium | Keep publish-service integration and Windows amd64 as explicitly deferred follow-ups.                 |

## S2 exit criteria

- [x] Verified HTTPS registry consumption works for locked direct and nested
      dependencies.
- [x] Supported Unix release artifacts package, verify, install, and run from
      outside the repository.
- [x] CI and local acceptance gates are green.
- [x] Release and installer documentation matches actual behavior.
- [x] Remaining debt is documented, with Windows amd64 explicitly deferred.

## Closeout

- Acceptance run: `bash scripts/release-acceptance.sh` passed on 2026-07-21.
- Network smoke remains opt-in because it depends on the published CDN; the
  offline gate is the required local/PR gate.
- The S2 implementation and local acceptance gate are complete; publishing a
  release remains a maintainer action.

## Open questions

- The HTTPS registry endpoint and authentication policy must be confirmed before
  implementing `aura publish`; publishing is not an S2 blocker without that
  external contract.
- A native macOS amd64 acceptance runner may be needed for runtime execution;
  cross-build validation alone is insufficient for full confidence.
