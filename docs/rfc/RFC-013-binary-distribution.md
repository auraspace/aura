# RFC-013: Binary Distribution

| Field        | Value                    |
| ------------ | ------------------------ |
| **RFC**      | 013                      |
| **Title**    | Binary Distribution      |
| **Status**   | Accepted                 |
| **Layer**    | Toolchain                |
| **Authors**  |                          |
| **Created**  | 2026-07-15               |
| **Updated**  | 2026-07-21               |
| **Estimate** | 20–30 pages              |
| **Depends**  | RFC-000, RFC-008         |
| **Blocks**   | RFC-012 (toolchain cmds) |

---

## 1. Abstract

This RFC covers **how Aura ships**: platform matrix for the **toolchain** and for **user applications**, installers, archive layouts, **code signing**, checksums, self-update, and packaging of **single-file app binaries**. Default application deploy remains **one executable** produced by `aura build`.

**Toolchain today (2026-07-23, S2):** **`v0.1.0-alpha`** is published for Linux amd64 and macOS amd64/arm64. `curl …/install.sh` installs into versioned `$AURA_HOME` (`~/.aura/versions/<ver>`, `current`, `avm`), while `cargo install --path crates/aura-cli` uses the embedded `aura_rt.c`. Tag `v*` produces multi-OS tarballs, per-archive checksums, an aggregate `SHA256SUMS`, and a detached minisign signature; production tags fail closed unless both signing secrets are configured and the workflow verifies the signature. Windows amd64/arm64 remain tier2 policy targets and are not yet published.

## 2. Motivation

### 2.1 Problem statement

A language that compiles to native code still fails users if the toolchain is hard to install or app artifacts are multi-file messes. Distribution is part of the product.

### 2.2 Why now

Build outputs (RFC-008) and CLI (RFC-012) need install and release contracts.

### 2.3 Success metrics

| Metric            | Target                             |
| ----------------- | ---------------------------------- |
| Toolchain install | One command or single archive      |
| App ship          | One file per OS/arch               |
| Integrity         | Checksums + signatures on releases |

## 3. Goals

- Document supported OS/arch matrix for v1.
- Standard release artifact naming.
- Signing & verification story.
- Self-update for toolchain (optional but designed).
- Clear guidance for app distribution (not an app store).

## 4. Non-goals

- App Store / Play Store submission pipelines.
- WASM distribution as v1 primary.
- Guaranteeing notarization on day-one for all platforms (track per-OS requirements).

## 5. Prior art & alternatives

| System          | Notes                  | Take                      |
| --------------- | ---------------------- | ------------------------- |
| Rustup          | Toolchain install      | Inspiration               |
| Go              | Single static binaries | App ship model            |
| Node installers | Multi-file             | Contrast                  |
| Docker-only     | Popular                | Compatible, not exclusive |

## 6. Design

### 6.1 Platform matrix (v1)

The machine-readable policy is [`scripts/release-targets.tsv`](../../scripts/release-targets.tsv). `required` rows are built, checksum-checked, and installer-supported. `tier2` rows are explicit policy commitments only and must not be described as shipped until native build and install evidence exists.

The current required artifact suffixes are `linux-amd64`, `darwin-arm64`, and
`darwin-amd64`; `linux-arm64`, `windows-amd64`, and `windows-arm64` are tier2
policy rows and have no release assets yet.

| OS          | Arch                              |
| ----------- | --------------------------------- |
| Linux (gnu) | amd64, arm64                      |
| macOS       | amd64, arm64                      |
| Windows     | amd64, arm64 (tier2; not shipped) |

Musl/static Linux: stretch goal for super-portable apps.

### 6.2 Toolchain artifacts

Naming:

```text
aura-toolchain-{version}-{os}-{arch}.tar.gz
aura-toolchain-{version}-{os}-{arch}.zip   # windows
```

Contents:

```text
bin/aura
lib/…          # if needed
share/…        # licenses, man
components…    # optional cross libs
```

### 6.3 Application artifacts

- Default: `target/release/<bin>` single executable.
- Optional: `aura pack` produces versioned tarball with binary + LICENSE + README (still one primary bin).
- Embed version via build: `aura build --release` sets version from manifest.

### 6.4 Install methods

| Method                      | Audience                         |
| --------------------------- | -------------------------------- |
| Install script (curl \| sh) | Dev machines (document risks)    |
| Package managers            | brew, apt, winget—when available |
| Manual archive              | Airgapped                        |
| `aura toolchain install`    | Self-management post-bootstrap   |

### 6.5 Integrity & signing

- Publish `SHA256SUMS` for every release.
- Sign sums with release key (**minisign** for simplicity of offline verify; cosign optional later for provenance). Production release tags fail closed when signing material is absent; unsigned rehearsals must not be promoted to a release.
- Windows Authenticode / macOS notarization: platform-specific checklist.

### 6.6 Self-update

- `aura toolchain upgrade` checks release API, verifies signature, replaces binaries atomically.
- Respects offline mode.
- Channels: `stable`, `beta`, `nightly` (optional).

### 6.7 Container story

- Official slim images optional; still demonstrate copying single static binary into `FROM scratch` / distroless when linked appropriately.

### 6.8 Examples

```text
# install toolchain (illustrative)
curl -fsSL https://aura.dev/install.sh | sh

aura version
aura build --release -o greeter
scp greeter host:/usr/local/bin/
```

### 6.9 Error model / edge cases

| Case                 | Behavior                       |
| -------------------- | ------------------------------ |
| Signature fail       | Abort update                   |
| Partial download     | Atomic replace only on success |
| Unsupported platform | Clear message                  |

### 6.10 Compatibility & migration

- Toolchain versions independent of language editions.
- App binaries: no guarantee to run under different runtime major if dynamically linked—**static default** avoids this.

## 7. Open questions

| #   | Question           | Options        | Owner   | Status                                                                                                                                       |
| --- | ------------------ | -------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Signing technology | minisign first | Dist    | **Resolved** (cosign later optional)                                                                                                         |
| 2   | Musl tier          | tier1 / tier2  | Dist    | **Resolved** — musl **tier2** initially                                                                                                      |
| 3   | Hosting URL / CDN  |                | Project | **Resolved** — toolchain via GitHub Releases + docs site; **packages** also GitHub-backed (index repo + Release `.crate` assets) per RFC-005 |
| 4   | Windows arm64 tier |                | Dist    | **Resolved** — Windows arm64 **tier2**                                                                                                       |

## 8. Rationale & trade-offs

Single-file apps maximize operational simplicity. Toolchain archives + checksums are table stakes for professional languages. Self-update improves DX but increases supply-chain criticality—hence signatures. Cost: platform signing bureaucracy (Apple/Microsoft) is ongoing ops work.

## 9. Unresolved / future work

- Deb/rpm official packages
- SBOM generation on release
- Provenance (SLSA) levels

## 10. Security & safety considerations

- Install scripts are sensitive—pin versions, document checksum verification alternative.
- Release keys offline / HSM policy.
- Rollback path for bad toolchains.
- Do not auto-exec downloaded app code.

## 11. Implementation plan (optional)

| Phase | Scope                       | Exit criteria      |
| ----- | --------------------------- | ------------------ |
| D0    | Manual archives + checksums | GitHub releases CI |
| D1    | Install script + matrix     | 6 platform builds  |
| D2    | Sign + self-update          | Verified upgrade   |

The alpha gate validates the target manifest against the workflow, package
script, installer, and this RFC before assets are built.

For alpha acceptance, a native target report is valid only when it records a
successful binary execution on the matching OS/architecture; cross-file
reports record format inspection and explicitly state that execution did not
occur. This is intentionally weaker than foreign-host acceptance. Linux arm64
and Windows promotion still require native package/install/run evidence on
their declared runners, and a production release still requires an actual
tag-triggered signature verification using configured GitHub credentials.

## 12. References

- Go install/dist; rustup
- RFC-000, RFC-008, RFC-012

---

## Changelog

| Date       | Author | Change                                                                                   |
| ---------- | ------ | ---------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock tiers + hosting; Status → **Accepted**                                              |
| 2026-07-16 |        | Status → **In Review** — Review: single-file + minisign locked; tiers/hosting still open |
| 2026-07-15 |        | Initial skeleton                                                                         |
| 2026-07-15 |        | Solid draft: matrix, signing, single-file apps                                           |
| 2026-07-15 |        | Lock minisign for release signatures                                                     |
