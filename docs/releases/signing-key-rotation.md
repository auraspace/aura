# Signing-Key Rotation and Recovery

This runbook covers rotation of the release Minisign key without accepting an
unsigned or unexpectedly signed artifact.

## Rotation

1. Generate a new offline key pair on the release workstation:
   `minisign -G -p aura-release-next.pub -s aura-release-next.sec`.
2. Verify the new public key and fingerprint using a second trusted operator.
3. Add the new public key to the release verification fixture and CI secret
   configuration before publishing an artifact signed by it.
4. Publish one release with both the old and new verification records available
   to consumers; sign the artifact with the new key.
5. Run `bash scripts/release-acceptance.sh` and the platform acceptance workflow.
6. Remove the old verification record only after the overlap release has been
   retained and its rollback artifact remains available.

## Compromise or Lost Secret

- Revoke the compromised key in the release metadata and stop publishing from
  the affected workstation.
- Restore the last known-good artifact using
  [the rollback runbook](rollback-runbook.md).
- Generate a replacement key on a separate offline workstation and repeat the
  overlap process above.
- Record the incident, affected tags, and replacement fingerprint in the
  release notes; never replace a published signature in place.

Private signing keys are never committed, uploaded to CI logs, or placed in a
package archive. Public-key changes are reviewed as release-policy changes.
