# Workstream 06: Profiles, Cache, and Targets

Owner: Tooling + Compiler. Scope: 8 tasks.

## P1. Profile schema

**Objective:** Define `dev`, `test`, and `release` configuration.
**Implementation status:** Complete. Profiles now normalize deterministic
defaults, support inheritance, validate known keys and values, reject unknown
profiles/conflicting aliases, and preserve minimal manifest compatibility.
**Checklist:**

- [x] Define defaults, inheritance, validation, and unknown-key behavior.
- [x] Preserve compatibility with minimal existing manifests.
- [x] Document optimization, debug, detector, panic, backend, and linker knobs.
      **Acceptance:** Every profile has a deterministic normalized configuration.
      **Verification:** Parse valid, missing, conflicting, and invalid manifests.
      **Dependencies:** B1, C1–C3.

## P2. Profile behavior

**Objective:** Make profile selection affect compilation predictably.
**Implementation status:** Complete for the C backend. Normalized profile
settings now control optimization, debug info, LTO, sanitizer detector flags,
and linker selection; effective settings are retained in `BuildIdentity`.
Release defaults keep detector instrumentation disabled.
**Checklist:**

- [x] Apply optimization, debug information, LTO, detector, and exception policy.
- [x] Record effective settings in artifact metadata.
- [x] Keep release detector-free by default.
      **Acceptance:** Equivalent source built under each profile has explainable
      differences and stable behavior.
      **Verification:** Compare flags, metadata, size, and runtime behavior.
      **Dependencies:** P1, B3.

## P3. Cache key schema

**Objective:** Prevent stale artifact reuse.
**Implementation status:** Complete for the codegen artifact cache. Keys include
compiler/backend/ABI/target/profile/features/source/imports/lockfile/toolchain
inputs, use length-delimited canonical serialization, and are SHA-256 hashed.
**Checklist:**

- [x] Include compiler, backend, ABI, target, profile, features, source,
      imports, lockfile, and relevant toolchain inputs.
- [x] Define normalized serialization and hash algorithm.
- [x] Identify which changes invalidate which stages.
      **Acceptance:** Every relevant input change invalidates the correct artifact.
      **Verification:** Mutate one input at a time and inspect cache traces.
      **Dependencies:** P1, B5, A1.

## P4. Cache storage

**Objective:** Make cache writes deterministic and recoverable.
**Implementation status:** Complete for the standalone artifact cache API.
Entries publish artifact and metadata through temporary files and renames;
missing, mismatched, or corrupt entries are discarded before reuse.
**Checklist:**

- [x] Use atomic publication and safe concurrent writers.
- [x] Validate checksums and metadata before reuse.
- [x] Discard partial or corrupt entries automatically.
      **Acceptance:** Interrupted or concurrent builds never publish unusable entries.
      **Verification:** Run interrupted-write, corruption, and parallel-build tests.
      **Dependencies:** P3.

## P5. Clean behavior

**Objective:** Provide safe, scoped artifact cleanup.
**Checklist:**

- [ ] Define project, target, profile, and shared-cache scopes.
- [ ] Refuse ambiguous or unsafe cleanup requests.
- [ ] Preserve unrelated projects and user configuration.
      **Acceptance:** Cleanup removes only the selected scope and is observable.
      **Verification:** Run clean tests with multiple projects and profiles.
      **Dependencies:** P4.

## P6. Target capability validation

**Objective:** Validate supported Linux/macOS builds before compilation.
**Checklist:**

- [ ] Check target, linker, sysroot, runtime, system libraries, and backend.
- [ ] Distinguish native execution from cross-compilation.
- [ ] Report unsupported targets and supported alternatives.
      **Acceptance:** Unsupported targets fail before partial artifacts are emitted.
      **Verification:** Run preflight on supported and deliberately incomplete hosts.
      **Dependencies:** B4, C2.

## P7. Single executable linking

**Objective:** Produce a self-contained executable for each supported target.
**Checklist:**

- [ ] Embed required runtime and standard components.
- [ ] Apply profile and target linker settings.
- [ ] Preserve symbols/debug data according to profile.
      **Acceptance:** Installed output runs without a separate runtime installation.
      **Verification:** Build, archive, install, and execute smoke fixtures.
      **Dependencies:** P2, P6, A1.

## P8. Reproducible build matrix

**Objective:** Prove cold/warm and cross-host build claims.
**Checklist:**

- [ ] Run repeated cold and warm builds for each profile and target.
- [ ] Compare checksums, metadata, and runtime behavior.
- [ ] Record native versus compile-only results.
      **Acceptance:** Release claims are backed by repeatable evidence.
      **Verification:** Execute the matrix from clean checkouts on supported hosts.
      **Dependencies:** P3–P7.
