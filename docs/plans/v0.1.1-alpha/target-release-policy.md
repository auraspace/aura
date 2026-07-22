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

`darwin-*` is the artifact target spelling for macOS. Windows and other
architectures are outside this release contract.

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

Minisign signing of `SHA256SUMS` is optional. When the signing secret and
public key are configured, the workflow signs and verifies the manifest and
publishes `SHA256SUMS.minisig` plus `minisign.pub`. No release claim depends
on minisign being configured.

## Workflow contract

Pull-request CI packages and checksum-verifies all three targets. Tag release
CI uses the same matrix, checks the exact versioned filenames and adjacent
checksums, verifies the checksums again in the release job, and only then
uploads the assets to GitHub Releases.
