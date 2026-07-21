# C13s — Signing / notarization design note

| Field      | Value                                                                               |
| ---------- | ----------------------------------------------------------------------------------- |
| **Opened** | 2026-07-21                                                                          |
| **Slice**  | C13s (docs only — option **B**)                                                     |
| **After**  | C12s install smoke + Unix release matrix; RFC-013 §6.5                              |
| **Goal**   | Record how Aura will sign releases **without** shipping notarized installers in C13 |

## Status

**Design note only.** No secrets, no CI signing steps, no Apple/Microsoft account setup, no Windows job.

**C13s choice:** **B** — short signing/notarization design note.  
**Not chosen:** **A** — best-effort Windows CI (`continue-on-error`). C12s already skipped Windows as a release gate; `.github/workflows/ci.yml` and `release.yml` remain Unix-only (Linux amd64, macOS arm64/amd64). A non-blocking Windows job is still valuable later, but integrity of published artifacts is the higher Dist/DX gap for pre-1.0.

## Today (alpha)

| Piece                         | State                                                                                        |
| ----------------------------- | -------------------------------------------------------------------------------------------- |
| Release tarballs              | Tag `v*` → `release.yml` → GitHub Release assets                                             |
| Per-archive checksum          | `scripts/package-release.sh` writes `*.tar.gz.sha256` (local + CI artifact)                  |
| Aggregated `SHA256SUMS`       | Not published as a single signed manifest yet                                                |
| minisign / cosign / sigstore  | Not wired                                                                                    |
| macOS notarization / stapling | Not done (unsigned CLI; Gatekeeper may warn on first run of downloaded binary)               |
| Windows Authenticode          | N/A until Windows tarballs / installers ship                                                 |
| Installer trust model         | `curl \| bash` from site CDN; script source is monorepo `scripts/install.sh` (document risk) |

Users can verify a single archive with the adjacent `.sha256` file if they download both; there is no offline **signed** sum file or key fingerprint in docs yet.

## Target model (RFC-013 aligned)

Layer integrity so each step is optional until ops cost is justified:

```text
1. Checksums     every release asset has SHA-256
2. Signed sums   one SHA256SUMS (+ .minisig) per release
3. Install path  install.sh / avm verify sum (+ signature) before unpack
4. OS trust      macOS notarize; Windows Authenticode (when Windows ships)
5. Provenance    optional cosign/sigstore attestations for supply-chain tooling
```

Prefer **minisign** for the first signed layer (RFC-013 §6.5): small offline verify story, no heavy cloud KMS required for dogfood, public key can live in the repo and install docs.

## Phased checklist

### Phase 0 — already true / cheap

- [x] Per-tarball `.sha256` from `package-release.sh`
- [ ] Publish a root-level `SHA256SUMS` (or `SHA256SUMS.txt`) on each GitHub Release covering all assets
- [ ] Document verify one-liner in [install.md](../guide/install.md) (checksum only)

### Phase 1 — signed sums (first real signing work)

- [ ] Generate long-lived **minisign** keypair offline; store secret outside the repo (1Password / org secret store)
- [ ] Publish public key in-repo (e.g. `keys/minisign.pub`) and fingerprint on the website install page
- [ ] Release workflow: after all matrix artifacts land, job aggregates sums → `minisign -Sm SHA256SUMS`
- [ ] Attach `SHA256SUMS` + `SHA256SUMS.minisig` to the GitHub Release
- [ ] `install.sh` / `install-smoke.sh`: optional `AURA_VERIFY=1` (default on for non-CI?) — fetch sums + sig, verify, then unpack
- [ ] Document: `minisign -Vm SHA256SUMS -p keys/minisign.pub` then `sha256sum -c`

**Non-goals for Phase 1:** notarization, Authenticode, changing tarball layout, Windows matrix.

### Phase 2 — macOS notarization (when distribution friction warrants it)

Apple’s path for a **command-line tool** (not a `.app` bundle):

1. **Developer ID Application** certificate (Apple Developer Program).
2. Sign the `aura` binary (and any shipped dylibs, if any — today: none) with `codesign --options runtime --timestamp`.
3. Zip/tarball the signed binary (or sign before `tar` in the darwin matrix cells).
4. `notarytool submit` + wait; staple is for disk images/apps — for a raw CLI inside a tarball, **notarization of a zip of the binary** is the usual pattern; users still download our `.tar.gz` of an already-notarized tree.
5. Store Apple ID / team ID / app-specific password or API key as GitHub Actions secrets; never in git.
6. Document Gatekeeper expectations for `curl | bash` vs manual download.

**Cost drivers:** Apple membership, secret rotation, brittle CI on Apple side outages, hardened runtime entitlements if we ever need special ones (Aura alpha should need none).

Defer until: (a) unsigned downloads are a support burden, or (b) we ship `.pkg` / Homebrew cask.

### Phase 3 — Windows (with or after a Windows artifact)

1. Add Windows **build** to release matrix (amd64 first; arm64 tier2 per RFC-013).
2. **Authenticode** sign `aura.exe` with an org code-signing cert (EV preferred for reputation; standard OV possible).
3. Prefer Azure/GitHub OIDC → cloud HSM over committing `.pfx` to runners when possible.
4. Optional: `winget` / `scoop` later — still need a signed binary.
5. Best-effort **Windows CI** job (`continue-on-error: true`) can land independently of signing; it does not replace Phase 1 sums.

### Phase 4 — optional provenance

- cosign keyless (OIDC) attestations on release blobs for consumers who already verify sigstore.
- Keep minisign as the **human/offline** path; cosign is additive, not a replacement for install.sh.

## Trust & threat model (short)

| Threat                          | Mitigation                                                                                                                             |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Tampered GitHub Release asset   | Signed `SHA256SUMS`; pin minisign pubkey out-of-band                                                                                   |
| Compromised CDN `install.sh`    | Script embeds or fetches pubkey; checksum of script itself is weak alone — prefer versioned script from same signed release or git tag |
| Compromised CI signing key      | Offline minisign key for Phase 1; short-lived OIDC for cosign later                                                                    |
| “Trust on first use” curl\|bash | Document risk; offer manual tarball + verify path                                                                                      |

Alpha remains **honest about TOFU**: the design goal is verifiable releases, not perfect bootstrap from zero trust.

## Explicit non-goals (this note / C13)

- Implementing notarization, Authenticode, or minisign in CI
- Purchasing certificates or creating Apple/Microsoft accounts
- Windows CI matrix as a merge gate
- Changing `install.sh` behavior in this slice (pointer only)
- Self-update / `aura toolchain upgrade` signature verify (depends on Phase 1)

## Recommended next slice (post-C13)

1. **Phase 0–1 only:** aggregate `SHA256SUMS` + minisign in `release.yml` + verify hook in `install.sh`.
2. Separately: optional best-effort Windows **compile** job (former C13s option A) without making it a release gate.
3. Notarization only when macOS support load justifies Apple Program + secrets ops.

## Related

- [RFC-013 Binary distribution](../rfc/RFC-013-binary-distribution.md) §6.5 Integrity & signing
- [docs/guide/install.md](../guide/install.md)
- [docs/releases/0.1.0-alpha.md](../releases/0.1.0-alpha.md)
- `.github/workflows/release.yml`, `scripts/package-release.sh`, `scripts/install.sh`
- Plan: [2026-07-21-next-20-c13a-c13t.md](./2026-07-21-next-20-c13a-c13t.md) (C13s)
