# C2: Target and Release Policy

This policy defines the v0.1.1-alpha release targets and the claims made by
the CI and release workflows.

## Supported targets

The release matrix contains exactly these Unix targets:

| Target ID      | Platform     | Rust target                | Build mode                   |
| -------------- | ------------ | -------------------------- | ---------------------------- |
| `linux-amd64`  | Linux x86_64 | `x86_64-unknown-linux-gnu` | Native on `ubuntu-latest`    |
| `darwin-arm64` | macOS arm64  | `aarch64-apple-darwin`     | Native on `macos-14`         |
| `darwin-amd64` | macOS x86_64 | `x86_64-apple-darwin`      | Cross-compiled on `macos-14` |

`darwin-*` is the artifact target spelling for macOS. Linux arm64 and Windows
targets are policy-only until their native artifact and installer acceptance
evidence exists; they are not release claims in this alpha. The explicit
non-claims are checked by JSON fixtures under
`scripts/fixtures/target-policy/`.

If a supported target fails preflight or release acceptance, it is removed
from the published matrix and release assets until its native/cross acceptance
gate passes again. The failure remains recorded as a blocked target in the
contract matrix rather than being silently treated as unsupported.

Native means the runner's host architecture matches the artifact and the
packaged executable is smoke-tested by running `aura version` and a generated
hello project. Cross means compilation targets another architecture; CI must
validate the binary format and architecture, but does not claim execution on
the build runner.

## Artifact and integrity requirements

For a tag `v{version}`, each target publishes exactly:

```text
aura-{version}-{target}.tar.gz
aura-{version}-{target}.tar.gz.sha256
```

Every archive must have its adjacent SHA-256 checksum, and the release job
must verify all three checksums before publishing. The release also publishes
an aggregate `SHA256SUMS` manifest covering the release assets.

Minisign signing of `SHA256SUMS` is required for production tags. The workflow
fails closed unless the signing secret and matching public key are configured,
then signs and verifies the manifest and publishes `SHA256SUMS.minisig` plus
`minisign.pub`.

## Workflow contract

Pull-request CI packages and checksum-verifies all three targets. Tag release
CI uses the same matrix, checks the exact versioned filenames and adjacent
checksums, verifies the checksums again in the release job, and only then
uploads the assets to GitHub Releases.
