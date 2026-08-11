# Migration Guide: 0.1 Alpha Releases

Aura follows SemVer. Alpha releases may still require source changes, so every
breaking change is called out here before a tag is published.

## Package and CLI

- Keep `aura.lock` under version control; refresh it with `aura update` and do
  not hand-edit checksums or immutable Git revisions.
- Use `aura toolchain list`, `current`, and `switch` for installed versions.
- Machine consumers should use `--format json`; usage errors are exit `2` and
  operational failures are exit `1` unless the command documents a domain code.

## Language and runtime

- `ref T` remains a scoped, immutable, non-null borrow. Clone or materialize an
  owner before returning, capturing, sending, or crossing `await`.
- Task cancellation and deadlines are observable outcomes; callers must handle
  `Cancelled` and timeout failures rather than treating them as success.
- Password records created by `std.crypto.hashPassword` are versioned PBKDF2
  records and must be verified with `std.crypto.verifyPassword`.

## Release procedure

Run `bash scripts/release-acceptance.sh` and
`bash scripts/tests/website-bundle.sh` before publishing a tag. If an artifact
fails verification, follow `docs/releases/rollback-runbook.md`; the active
version must remain unchanged until a verified replacement is ready.
