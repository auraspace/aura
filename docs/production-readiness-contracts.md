# Production Readiness Contracts

This document records the release contracts that are independent of a specific
backend or host. A contract is only considered implemented when the associated
runtime/compiler tests and platform gates also pass.

## Package and Lockfile

`aura.lock` is a deterministic, repository-owned format. Readers must accept
the current schema and one previous schema for the same major toolchain; a
future incompatible schema must fail with an actionable version error. Entries
are sorted by package name and immutable origins include source, version,
revision, and checksum. Lock refresh is transactional: a failed fetch or
verification cannot modify the manifest, lockfile, cache, or active toolchain.

Workspaces are not implied by a lockfile. A root manifest declares
members = ["relative/member"]; member manifests are loaded independently, and
graph inspection exposes the workspace root and all members in declaration
order. Commands that compile a package still require an explicit member path.

## Test Conventions

- Unit tests stay beside the Rust or runtime component they exercise.
- Integration tests live under `runtime/tests`, `scripts/tests`, or a crate's
  integration-test directory and must be runnable without network access.
- Corpus fixtures are executable when their category says `run` and negative
  fixtures must assert a stable diagnostic code.
- Async tests must cover success, failure, cancellation, cleanup, and repeated
  owning joins when the operation crosses a suspension point.
- CI-compatible reports use the existing JSON test report contract; future
  JUnit/LCOV exporters must preserve package, case, status, duration, and
  diagnostic identity.

## Transport Scope

The production transport contract is POSIX/Windows TCP and TLS with HTTP/1.1,
bounded request/response bodies, explicit deadlines, and cancellation cleanup.
HTTP/2, HTTP/3, QUIC, WebSockets, compression, multipart parsing, and chunked
trailers beyond the documented request snapshot are explicitly out of scope
for the current release and require a separate RFC plus corpus and native-host
coverage before being advertised.

The capability matrix is therefore:

| Capability           | Current contract                                    | Evidence requirement                  |
| -------------------- | --------------------------------------------------- | ------------------------------------- |
| TCP                  | Supported, nonblocking, bounded binary I/O          | Native runtime and sanitizer fixtures |
| TLS                  | Supported through the configured native TLS adapter | FFI and platform acceptance           |
| HTTP/1.1             | Supported within documented bounds                  | HTTP corpus and runtime smoke         |
| HTTP/2, HTTP/3, QUIC | Not in current release scope                        | New RFC and compatibility matrix      |

## Runtime Limits and Cancellation

Bounded APIs must reject oversize input before allocation. The supported release
limits are the constants enforced by the runtime/parser fixtures; callers must
use streaming or package-level chunking when an operation exceeds those limits.
Cancellation is cooperative: a pending operation observes cancellation at its
next scheduler/reactor boundary, closes owned file/socket resources, releases
pending channel values, and reports `Cancelled`. Deadlines are absolute
monotonic deadlines; expiry follows the same cleanup path and reports the
operation's timeout error. No API may silently convert cancellation into
success.

## Release Quality Thresholds

The release gate requires zero failing tests, zero clippy warnings, zero
whitespace errors, all executable corpus fixtures to pass, and sanitizer jobs
to pass on every supported native matrix entry. Coverage thresholds are not
claimed until source instrumentation and reproducible LCOV export are shipped.

## Versioning and Migration

Toolchain, standard library, and CLI versions follow SemVer. Breaking language,
ABI, lockfile, or CLI changes require a major version. Additive APIs are minor;
bug fixes and diagnostics-only corrections are patch changes. Deprecations must
name a replacement, record the first deprecated version, and remain available
for at least one minor release unless a security issue requires earlier removal.

Every breaking release publishes a migration guide covering source changes,
lockfile changes, runtime/ABI changes, CLI changes, and rollback guidance.
