# Release Rollback Runbook

Use this procedure when a published Aura toolchain is defective or its
verification metadata is compromised.

## Preconditions

- Identify the last known-good immutable Git tag and release artifact.
- Confirm its checksum and Minisign signature with the current verification
  fixture.
- Freeze publication of newer artifacts until the incident owner approves the
  rollback.

## Procedure

1. Run `bash scripts/release-acceptance.sh --network` against the known-good
   release from a clean machine.
2. Restore the release pointer/CDN metadata to the known-good immutable tag;
   never overwrite an existing archive or checksum file.
3. Verify the installer, update path, rollback path, and offline cache using
   `bash scripts/install-smoke.sh --from-release` and the local release fixture.
4. Publish an incident note naming the withdrawn version, restored version,
   affected platforms, and required user action.
5. Revoke compromised signing metadata if applicable and follow
   [the key-rotation runbook](signing-key-rotation.md).
6. Keep the withdrawn artifact available for forensic verification, but prevent
   the installer and update service from selecting it.

## Exit Criteria

Rollback is complete only when all supported release targets pass their native
acceptance jobs, checksum/signature verification passes, and a clean-machine
install plus offline-cache smoke has produced a runnable `aura` binary.
