# Workstream 09: Registry, Publish, and Self-update

Owner: Tooling + Release. Scope: 8 tasks.

## U1. Package/archive contract

**Objective:** Define a verifiable artifact exchanged with registries and users.
**Implementation status:** Partial. The package module now builds deterministic
gzip/tar source archives rooted at `name-version/`, with sorted repository
paths, normalized tar ownership/mode/timestamp metadata, safe path validation,
and a lowercase SHA-256 helper. Manifest/dependency validation, signatures,
target-specific metadata, and publish orchestration remain deferred.
**Checklist:**

- [x] Define identity, version, source inclusion, target naming, and archive
      layout for the deterministic source archive primitive; full manifest
      validation remains deferred.
- [x] Define checksum and reproducibility rules for the archive primitive;
      signatures and release metadata remain deferred.
- [ ] Define compatibility and rejection behavior.
      **Acceptance:** The same package input produces the same verified archive.
      **Verification:** Compare repeated archives and malformed metadata cases.
      **Dependencies:** C1–C3, P1–P8.

## U2. Registry client

**Objective:** Consume and publish registry data through a stable protocol.
**Implementation status:** Partial. The client already reads local/offline
fixtures and HTTPS metadata/tarballs with bounded timeouts; transient transport
and 5xx failures now retry at most three times, while 4xx responses fail
immediately. HTTPS requests may carry an optional bearer token from
`AURA_REGISTRY_TOKEN`; upload and stable error taxonomy remain open.
**Checklist:**

- [ ] Support configuration and upload.
- [x] Support HTTPS fetch, bounded timeout/retry, and optional bearer
      authentication via `AURA_REGISTRY_TOKEN`; upload remains open.
- [ ] Map HTTP status, transport, auth, and validation failures to stable errors.
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
      signatures and broader registry tamper policy remain open.
      **Acceptance:** A locked graph resolves identically online and from warm cache.
      **Verification:** Run conflict, missing, checksum, cycle, and offline cases.
      **Dependencies:** U2, P3–P4.

## U4. Publish dry-run

**Objective:** Validate and preview publishing without network mutation.
**Implementation status:** Implemented as a bounded, read-only `aura publish
--dry-run` command. It validates the release manifest/version, source entries,
path dependencies, and locked registry dependency pins, then builds the U1
archive in memory and previews its size/checksum. It never resolves/fetches,
writes `aura.lock`, contacts a registry, or uploads. Release signatures remain
explicitly deferred until the signing primitive and key policy are defined.
**Checklist:**

- [x] Validate manifest, version, package contents, and dependencies.
- [x] Produce bounded archive/checksum preview; report signature as deferred
      rather than claiming an unsigned artifact is signed.
- [x] Show all validation errors before any upload operation; dry-run has no
      upload path and does not mutate registry or package state.
      **Acceptance:** Dry-run never mutates registry state.
      **Verification:** Compare preview with actual package and block invalid inputs.
      **Dependencies:** U1, U2.

## U5. Publish upload

**Objective:** Publish valid packages with safe failure behavior. The alpha
contract is deliberately minimal and fixture-oriented: `POST
/api/v1/publish` with the deterministic U1 archive as the
`application/gzip` body and `X-Aura-Package`, `X-Aura-Version`, and
`X-Aura-Sha256` headers. An optional `Authorization: Bearer` header follows
the existing registry client convention. A successful response is HTTP 201
with `{"status":"published","name":"…","version":"…","checksum":"…"}`.
No multipart format, index mutation protocol, signing, or production registry
compatibility is implied by this alpha endpoint.

**Implementation status:** Implemented as a bounded upload after U4 validation.
The client uses a 30-second connect/read/write timeout, retries transport and
5xx failures at most three attempts, and caps the archive at 64 MiB and the
receipt at 64 KiB. 4xx responses are not retried. Version conflict (409) and
authentication (401/403) are stable rejections. Exhausted transport failures
are `indeterminate`, because a POST may have reached the registry; the CLI
returns exit code 3 and never claims completion. The fixture server is the
authoritative focused test for this contract.

**Checklist:**

- [x] Upload archive and metadata in one registry request; HTTP 201 is the
      only completion acknowledgment (server-side atomicity remains the
      registry's responsibility).
- [x] Handle version conflicts, retries, auth, and partial/indeterminate
      failures without reporting a false success.
- [x] Return stable exit codes and machine-readable JSON results.
      **Acceptance:** A failed publish cannot leave a falsely complete release.
      **Verification:** Run local-registry success, duplicate, timeout, and retry tests.
      **Dependencies:** U3, U4.

## U6. Update discovery

**Objective:** Select a compatible update for the current installation.

**Implementation status:** Complete for metadata-only discovery. The registry
index selects the highest newer non-yanked release whose checksum, target, and
Aura toolchain bounds validate; revoked, unsupported, and no-update outcomes
are stable and explainable. Payload download, signature verification, and
activation remain U7.
**Checklist:**

- [x] Discover versions and filter by platform, architecture, and compatibility.
- [x] Verify metadata before downloading payloads.
- [x] Define no-update, unsupported, and revoked-version behavior.
      **Acceptance:** The selected update is compatible and explainable.
      **Verification:** Run version, target, metadata, and unavailable-update cases.
      **Dependencies:** U1, U2, P6.

## U7. Verified atomic self-update

**Objective:** Replace the active toolchain without corrupting a working install.
**Checklist:**

- [ ] Download to isolated temporary storage.
- [ ] Verify checksum and signature before activation.
- [ ] Replace atomically and retain rollback information.
- [ ] Preserve the old version after interruption or validation failure.
      **Acceptance:** No failed update changes the active executable.
      **Verification:** Inject download, checksum, signature, permission, and crash
      failures.
      **Dependencies:** U6, P7.

## U8. Release integration

**Objective:** Prove registry, publishing, updating, and target artifacts work
together.
**Checklist:**

- [ ] Publish a fixture release to a local registry.
- [ ] Install, verify, update, rollback, and execute it on Linux/macOS.
- [ ] Record checksums, versions, target, host, and outcome.
      **Acceptance:** The release workflow is reproducible from a clean installation.
      **Verification:** Run the full release acceptance stage.
      **Dependencies:** U5, U7, P8.
