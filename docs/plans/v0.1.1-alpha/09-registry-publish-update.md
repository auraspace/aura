# Workstream 09: Registry, Publish, and Self-update

Owner: Tooling + Release. Scope: 8 tasks.

## U1. Package/archive contract

**Objective:** Define a verifiable artifact exchanged with registries and users.
**Implementation status:** Complete for the direct-origin source contract. The package module now builds deterministic
gzip/tar source archives rooted at `name-version/`, with sorted repository
paths, normalized tar ownership/mode/timestamp metadata, safe path validation,
and a lowercase SHA-256 helper. The offline `aura-sig-v1` envelope now provides
versioned trusted-key verification, tamper rejection, and monotonic sequence
checks; production registry compatibility and target-specific metadata remain
deferred.
**Checklist:**

- [x] Define identity, version, source inclusion, target naming, and archive
      layout for the deterministic source archive primitive; full manifest
      validation remains deferred.
- [x] Define checksum and reproducibility rules for the archive primitive;
      offline signature metadata is covered by `aura-sig-v1`.
- [x] Define bounded compatibility and rejection behavior for archive metadata;
      production registry policy remains deferred.
      **Acceptance:** The same package input produces the same verified archive.
      **Verification:** Compare repeated archives and malformed metadata cases.
      **Dependencies:** C1–C3, P1–P8.

## U2. Registry/origin client

**Objective:** Consume package origins through a Go-compatible direct-VCS
contract. Publication is a Git tag push; an HTTP proxy is explicitly out of
scope for this plan.
**Implementation status:** Complete for direct Git origins and the legacy
metadata/archive compatibility path. Git dependencies resolve semver tags or
explicit revisions, pin the resolved commit and deterministic archive checksum,
fetch through the user's Git credential setup or environment-backed bearer
header, and support warm-cache offline reload. The proxy read-object boundary is
reserved but not served.
**Checklist:**

- [x] Support local/offline origin fixtures and bounded source/archive reads.
- [x] Support HTTPS fetch, bounded timeout/retry, and optional bearer
      authentication via `AURA_REGISTRY_TOKEN` for private origins.
- [x] Map HTTP status, transport, auth, and validation failures to stable
      bounded CLI outcomes.
- [x] Resolve direct Git tags/revisions, archive the selected source tree, and
      pin `source`, `version`, `rev`, and `checksum` in `aura.lock`.
- [x] Keep credentials out of manifests, lockfiles, argv, and surfaced errors.
- [x] Keep offline fixtures separate from network-required tests; local fixture
      tests and isolated HTTP mock-server tests are maintained independently.
      **Acceptance:** Registry operations are deterministic against a local fixture
      service and safe against malformed responses.
      **Verification:** Test success, auth failure, timeout, malformed data, and retry.
      **Dependencies:** U1, C3.

## U3. Dependency resolution

**Objective:** Resolve registry dependencies without violating lock/checksum rules.
**Implementation status:** Partial. Semver selection, deterministic transitive
registry resolution, lock/source/checksum validation, warm-cache reuse, and
clear conflict/missing/cycle/checksum failures are covered by the local fixture
suite. Cross-registry compatibility and broader tamper/signature policy remain
open.
**Checklist:**

- [x] Resolve versions and transitive dependencies deterministically.
- [x] Validate lockfile, source identity, checksum, and warm-cache state.
- [x] Report conflicts, missing packages, cycles, and checksum tampering clearly;
      production registry trust policy remains open.
      **Acceptance:** A locked graph resolves identically online and from warm cache.
      **Verification:** Run conflict, missing, checksum, cycle, and offline cases.
      **Dependencies:** U2, P3–P4.

## U4. Local publication validation

**Objective:** Validate and preview publishing without network mutation.
**Implementation status:** Retired from the public CLI. The deterministic
archive builder and local-origin acceptance fixture remain internal test
primitives. Package publication follows the Go-style origin contract: commit
the source, create an immutable `vX.Y.Z` Git tag, and push it to the origin.
Production artifact signing remains governed by the release workflow.
**Checklist:**

- [x] Validate manifest, version, package contents, and dependencies.
- [x] Produce bounded archive/checksum preview; unsigned dry-run output does not
      claim production signing, while signed registry metadata uses `aura-sig-v1`.
- [x] Keep the local-origin fixture deterministic and mutation-free.
      **Acceptance:** The fixture verifies the exact archive/checksum consumed
      by the registry client.
      **Verification:** Compare the materialized archive with the installed
      package and reject checksum mismatches.
      **Dependencies:** U1, U2.

## U5. Origin publication contract

**Objective:** Define how a valid package becomes visible at its authoritative
origin. The target architecture follows Go publication: a maintainer commits
the package and pushes an immutable `vX.Y.Z` tag to the package repository.
There is no required package Release asset, registry upload endpoint, index
repository, or proxy in this alpha contract.

The former HTTP upload fixture has been removed. Publication tests now
materialize the deterministic archive at a local origin and verify it through
the same metadata/checksum read path used by consumers.

**Implementation status:** Complete for the public origin contract. A package is
published by an ordinary immutable Git tag push; the client discovers tags,
resolves the commit, archives the source, and pins/verifies the resulting
identity. Proxy serving and checksum database implementation are intentionally
outside scope, with the read-object boundary versioned for future use.

**Checklist:**

- [x] Verify local-origin archive materialization and machine-readable acceptance evidence.
- [x] Define the origin tag publication workflow and immutable tag checks.
- [x] Define direct VCS tag discovery and source archive mapping.
      **Acceptance:** A published version is discoverable from the repository,
      resolves to one immutable commit/checksum identity, and cannot silently
      change after the lockfile is written.
      **Verification:** The offline tag/commit fixture and direct-origin
      round-trip acceptance cover the contract; the public-origin script is
      available for operator-run live rehearsal.
      **Dependencies:** U3, U4.

## U6. Update discovery

**Objective:** Select a compatible update for the current installation.

**Implementation status:** Complete for metadata-only discovery. The origin
selects the highest newer non-yanked release whose checksum, target, and
Aura toolchain bounds validate; revoked, unsupported, and no-update outcomes
are stable and explainable. Signed-index verification is available through the
explicit trusted-key API; payload activation remains U7.
**Checklist:**

- [x] Discover versions and filter by platform, architecture, and compatibility.
- [x] Verify metadata before downloading payloads.
- [x] Define no-update, unsupported, and revoked-version behavior.
      **Acceptance:** The selected update is compatible and explainable.
      **Verification:** Run version, target, metadata, and unavailable-update cases.
      **Dependencies:** U1, U2, P6.

## U7. Verified atomic self-update

**Objective:** Replace the active toolchain without corrupting a working install.

**Implementation status:** Complete for the bounded local/filesystem and HTTP
fixture contract. Payloads download into an isolated temporary directory,
checksums are verified before activation, activation uses a same-filesystem
rename, rollback metadata retains the previous executable, and failed download
or verification leaves the active version untouched. Signature verification
and cross-host native execution remain outside this host-bound slice.
**Checklist:**

- [x] Download to isolated temporary storage and verify the checksum before
      activation; signed metadata can be verified before update selection.
- [x] Replace atomically and retain rollback information.
- [x] Preserve the old version after interruption or validation failure.
      **Acceptance:** No failed update changes the active executable.
      **Verification:** Inject download, checksum, signature, permission, and crash
      failures.
      **Dependencies:** U6, P7.

## U8. Release integration

**Objective:** Prove registry, publishing, updating, and target artifacts work
together.
**Implementation status:** Implemented for the native Linux acceptance target.
The focused origin fixture materializes a deterministic source archive at a
local repository, verifies and installs the bytes into an
isolated cache, discovers a compatible Linux update, activates a checksum-
verified native executable, restores the retained rollback artifact, and runs
both release versions. The test emits a stable JSON evidence record containing
package/version/checksum, target, host, and outcome; set `AURA_U8_REPORT` to
persist that record. macOS native execution still requires a native host run.
**Checklist:**

- [x] Exercise deterministic fixture publication to a local origin.
- [x] Provide credential-safe rehearsal for a real public GitHub package via
      `scripts/public-origin-acceptance.sh` against the public `std.io`
      package at `auraspace/aura` tag `v0.1.1-alpha.5`.
- [x] Install and verify it, discover/activate an update, roll back, and
      execute both artifacts on Linux.
- [x] Record checksums, versions, target, host, and outcome in the acceptance
      evidence JSON.
      **Acceptance:** The release workflow is reproducible from a clean installation.
      **Verification:** Run `cargo test -p aura-cli
u8_local_origin_release_acceptance -- --nocapture`; use a native macOS
      host before making a macOS execution claim.
      **Dependencies:** U5, U7, P8.
