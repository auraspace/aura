# S3 — Production Release Execution

| Field      | Value                                                                                      |
| ---------- | ------------------------------------------------------------------------------------------ |
| **Opened** | 2026-07-21                                                                                 |
| **After**  | S2 production toolchain implementation                                                     |
| **Status** | Planned / release deferred                                                                 |
| **Goal**   | Execute and validate the first production-facing Aura release on the supported Unix matrix |

## Objective

S2 established the compiler, package-consumption, artifact, installer, and CI
foundations. S3 is the operational release slice: decide the release version,
publish verified artifacts, close the integrity story, and document the exact
support boundary for users.

S3 does not reopen the completed S2 implementation unless a release rehearsal
finds a concrete blocker.

## Release contract

Supported release targets remain:

- Linux amd64 (`x86_64-unknown-linux-gnu`)
- macOS arm64 (`aarch64-apple-darwin`)
- macOS amd64 (`x86_64-apple-darwin`)

Windows amd64, LLVM, async/tasks/channels, true borrow types, `Array` of
interfaces, and generic `HashMap<K,V>` remain outside this release.

## Phase 1: Release decision and rehearsal

### Task S3.1: Freeze release metadata

**Acceptance criteria:**

- [ ] Select the release version and status (`alpha`, `beta`, or stable).
- [ ] Update release notes, changelog, roadmap, and install documentation to
      use the same version and support claims.
- [ ] Confirm the Git tag, Cargo version, archive names, and installer version
      resolution agree.

**Verification:**

- [ ] `git diff --check`
- [ ] Review all release-facing docs for contradictory “published”, “pending”,
      or “deferred” claims.

**Dependencies:** None

### Task S3.2: Rehearse the complete release matrix

**Acceptance criteria:**

- [ ] Build one artifact for each supported target from the release tag.
- [ ] Verify every archive checksum and inspect archive contents.
- [ ] Install each native artifact outside the repository and run `aura
  version`, `aura new`, and `aura run`.
- [ ] Verify a failed or interrupted install does not replace the active
      version.

**Verification:**

- [ ] `bash scripts/package-release.sh` on the release commit.
- [ ] `bash scripts/install-smoke.sh --local-pkg` on a supported Unix host.
- [ ] Native macOS amd64 runtime smoke, not only cross-compiled file-type
      validation.

**Dependencies:** S3.1

## Phase 2: Artifact integrity

### Task S3.3: Publish aggregate checksums

**Acceptance criteria:**

- [ ] Generate one `SHA256SUMS` file covering all release assets.
- [ ] Attach the aggregate manifest to the GitHub Release.
- [ ] Document manual verification beside the installer path.

**Dependencies:** S3.2

### Task S3.4: Add signed release metadata

**Acceptance criteria:**

- [ ] Generate and protect a release signing key outside the repository.
- [ ] Publish the public key/fingerprint in a stable, reviewable location.
- [ ] Sign `SHA256SUMS` in CI and attach the signature to the release.
- [ ] Add opt-in signature verification to the installer without weakening the
      existing checksum failure behavior.

**Dependencies:** S3.3

## Phase 3: Publish and observe

### Task S3.5: Publish the release

**Acceptance criteria:**

- [ ] Push the release tag and confirm all three artifacts and metadata appear
      on the GitHub Release.
- [ ] Verify installation through the documented release URL, not a local path.
- [ ] Confirm the public website serves the matching install script and docs.
- [ ] Record rollback steps: hide/revoke the release, restore the prior
      `current` version, and mark the broken version clearly.

**Dependencies:** S3.4

### Task S3.6: Production smoke and support handoff

**Acceptance criteria:**

- [ ] Run a clean-host smoke for `new → run → build → test`.
- [ ] Run the dogfood `examples/wc` flow with forwarded CLI arguments.
- [ ] Record known limitations and first-response diagnostics for installer,
      C compiler, registry, and unsupported platforms.
- [ ] Move unresolved release issues into `agents/debts.md`.

**Dependencies:** S3.5

## Deferred follow-ups after S3

- Registry publishing, authentication, and `git=`/`github=` dependencies.
- Generic `HashMap<K,V>`.
- True borrow/ref semantics and `var` class/Array/Fun captures.
- `Array<Interface>`.
- Async/tasks/channels and LLVM backend.
- Windows artifacts and platform signing/notarization.

These are product roadmap items, not reasons to reopen S2’s completed toolchain
implementation unless they become explicit release requirements.

## S3 exit criteria

- [ ] Release metadata is internally consistent and frozen.
- [ ] All supported artifacts are built, checksummed, signed, and installable.
- [ ] At least one clean-host smoke passes for each native supported target.
- [ ] Release URL, installer, docs, and rollback procedure are verified.
- [ ] Known limitations are visible to users and tracked in technical debt.

## Risks

| Risk                                    | Impact | Mitigation                                                                                  |
| --------------------------------------- | ------ | ------------------------------------------------------------------------------------------- |
| Release artifact differs across runners | High   | Build from one tag, verify names/checksums, archive contents, and native smoke              |
| Unsigned or tampered download           | High   | Aggregate checksum manifest plus signed metadata and documented manual verification         |
| Installer activates a broken version    | High   | Atomic staging, isolated `AURA_HOME` rehearsal, and rollback procedure                      |
| Registry API is not ready               | Medium | Ship consuming locked packages only; keep publish/auth outside S3 critical path             |
| macOS Gatekeeper friction               | Medium | Document unsigned alpha behavior; schedule notarization only when support data justifies it |

## Related

- [S2 — Production Toolchain & Release Readiness](./2026-07-21-s2-production-toolchain.md)
- [C13s signing design note](./2026-07-21-c13s-signing-note.md)
- [RFC-013 Binary Distribution](../rfc/RFC-013-binary-distribution.md)
- [Release install guide](../guide/install.md)
- [Technical debt](../../agents/debts.md)
