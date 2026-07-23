# Release signing configuration

The release workflow always publishes `SHA256SUMS` covering the tarballs and
their adjacent `.sha256` assets. Production tags fail closed: minisign signing
and verification are required, and an unsigned rehearsal must not be promoted.

For a signed release, generate the minisign keypair offline and store the
private key only in the GitHub Actions secret
`AURA_MINISIGN_SECRET_KEY`. Store the matching public-key text in the
repository variable `AURA_MINISIGN_PUBLIC_KEY`. The workflow writes the secret
to a mode-600 temporary file, signs `SHA256SUMS`, verifies the signature with
the configured public key, and publishes `SHA256SUMS.minisig` plus
`minisign.pub`. Neither key is committed to the repository.

The release job also publishes `release-manifest.json`. It records the exact
required target artifacts, their SHA-256 values, and the native or cross-file
acceptance report used for each target. `scripts/validate-release-bundle.sh`
checks that manifest, the per-file checksums, the aggregate `SHA256SUMS`, and
the detached signature before the GitHub Release step can run. A local
fixture for the same contract is `scripts/tests/release-bundle.sh`.

The installer keeps checksum verification mandatory. To verify the signed
aggregate manifest, install minisign and opt in:

```sh
AURA_VERIFY_SIGNATURE=1 AURA_MINISIGN_PUBLIC_KEY_FILE=/path/to/minisign.pub \
  curl -fsSL https://aura.fadosoft.com/install.sh | bash
```

`AURA_MINISIGN_PUBLIC_KEY` may be used for inline configuration instead. If
neither explicit setting is supplied, the installer fetches `minisign.pub`
from the selected GitHub Release. Signature verification fails closed when
the manifest, signature, or trusted public key is unavailable or invalid.
