# Production-readiness release notes

## Evidence locked for the 2026-08 gate

The repository now has one repeatable repository gate:
`bash scripts/production-readiness-gate.sh`. It runs formatting, workspace
tests, clippy, corpus/compiler regressions, async coverage, FFI/runtime checks,
registry acceptance, and three configurable soak rounds. The following focused
gates are also part of the release contract:

- `bash scripts/release-acceptance.sh` performs an isolated offline package,
  checksum, rollback, install, AVM, corpus, sanitizer, and signing-policy
  rehearsal.
- `bash scripts/tests/cli-compatibility.sh` checks command discoverability,
  stable usage exit classes, toolchain JSON, and dependency-tree JSON.
- `bash scripts/tests/website-bundle.sh` runs site tests/prerendering and
  rejects any emitted JS/CSS asset over 500 KiB. Route, vendor, and syntax
  highlighting chunks are split in `site/vite.config.ts`.
- `bash scripts/tests/fuzz-smoke.sh` builds lexer, parser, manifest, and
  lockfile libFuzzer targets; no network or credentials are needed.

The package API exposes read-only workspace member discovery and recursive
workspace dependency graphs. The CLI exposes toolchain list/current/switch
commands for installed version-management layouts. These additions are
backward-compatible and do not change the `aura.lock` wire format.

## Platform matrix

CI executes the native sanitizer and FFI matrix on Linux amd64, Linux arm64,
and macOS arm64. Release packaging additionally validates macOS amd64 as a
cross-file artifact and records native/cross-file acceptance JSON. Leak
sanitization is enabled on Linux; macOS uses the host-supported ASan policy
(`detect_leaks=0`) because Apple Clang does not provide a reliable LSan runtime.
This is a platform policy, not an untracked exception.

Credential-safe fetch tests cover bearer-token redaction, embedded-credential
rejection, immutable Git revisions, HTTPS cache boundaries, SSH-origin parsing,
and fail-closed checksum/signature verification. Secrets are never written to
fixtures, logs, lockfiles, or generated reports.

## Type, macro, and runtime closure

Mutable `ref` is explicitly rejected in the MVP; nullable and nested references
are rejected before code generation. The sema suite contains negative tests for
returns, assignment escapes, field storage, lambda capture, await/spawn/channel
boundaries, nullable refs, mutable refs, and nested task storage. General async
ownership is executable-tested across branches, loops, repeated awaits, joins,
cancellation, channels, aggregate captures, and GC.

Procedural macros run through the versioned sandbox ABI with cleared
environment, no network, bounded source/output, timeout enforcement, path
confinement, root-plugin checksums, dependency-plugin rejection, expansion
origin spans, and deterministic generated-item ordering. Reproducibility is
covered by the macro protocol and package-loader tests.

The runtime provides bounded TCP/TLS/file/UDP/HTTP APIs and POSIX Unix-domain
socket coverage through the native reactor/socketpair fixtures. Password
derivation is PBKDF2-HMAC-SHA-256 with explicit iteration/length bounds and
constant-time comparison; API behavior and vectors are in `runtime/tests/crypto.c`.
