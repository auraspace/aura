# Security Review: 2026-08 Production Gate

## Scope

The review covers FFI handles and callbacks, generated C ownership/GC paths,
TLS verification and deadlines, Git/HTTPS/SSH package fetching, registry
checksums/signatures, proxy cache path safety, and procedural macro execution.

## Controls and evidence

- Generated C uses bounded allocations, explicit cleanup, sanitizer fixtures,
  task cancellation cleanup, and typed FFI pin/unpin boundaries.
- TLS and network APIs use bounded buffers, absolute monotonic deadlines, peer
  verification configuration, and cancellation cleanup.
- Fetching rejects credentials in origins, redacts bearer tokens, verifies
  immutable revisions/checksums/signatures, and writes caches atomically.
- Proxy paths are normalized and bounded; traversal, unsupported protocols,
  oversized objects, and cache conflicts fail closed.
- Macro plugins run with a cleared environment, network isolation, read-only
  source/plugin mounts, bounded protocol fields/output, timeout, and dependency
  provenance checks.

The executable evidence is wired into `scripts/release-acceptance.sh`,
`scripts/sanitizer-smoke.sh`, `scripts/tests/registry-release.sh`, and the Rust
package/sema/codegen test suites. Findings are release blockers when a gate
fails; no accepted exception is recorded for this review.
