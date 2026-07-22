# v0.1.1-alpha clean-host baseline

The clean-host baseline is the release evidence for the contract matrix. The
default run is offline and must not depend on a user's registry, Aura install,
or application cache.

## Supported host classes

| Host        | Target claim   | Execution claim                           |
| ----------- | -------------- | ----------------------------------------- |
| Linux amd64 | `linux-amd64`  | Native compile and runtime                |
| macOS arm64 | `darwin-arm64` | Native compile and runtime                |
| macOS arm64 | `darwin-amd64` | Cross-compiled artifact and metadata only |

Each run records the host OS/architecture, target, Rust/compiler version,
profile, C compiler, linker, environment requirements, command line, stage,
fixture, duration, status, and exit code. Reports are artifacts, not tracked
source files; summaries and classification changes belong in this document or
the associated release evidence.

## Offline baseline

```bash
cargo test --workspace
bash scripts/validate-alpha-contract.sh
bash scripts/alpha-harness.sh --offline --profile test --keep-temp
bash scripts/release-acceptance.sh --dry-run
```

The registry stage is reported as deferred/skipped unless `--network` is
explicitly supplied. A clean checkout must have a system C compiler and the
Rust toolchain declared by `Cargo.toml`; no application or registry cache is a
prerequisite.

## Network baseline

Run only after the offline baseline passes:

```bash
bash scripts/alpha-harness.sh --network --profile release --keep-temp
```

Network failures are classified separately from product failures and must
include the endpoint/stage and rerun command. A failed download, checksum, or
signature verification must never replace the currently active installation.

## Failure classification

- `product`: reproducible failure with the same fixture and environment.
- `environment`: missing compiler, linker, permission, or required tool.
- `flaky`: non-deterministic failure reproduced across repeated runs.
- `expected`: documented `partial`, `blocked`, or `deferred` matrix behavior.
