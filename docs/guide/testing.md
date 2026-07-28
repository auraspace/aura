---
title: Testing
section: Toolchain
order: 60
summary: Run, filter, race-check, and report @test functions with aura test.
---

# Testing

Aura’s MVP test runner is the `test` CLI verb ([RFC-011](/rfc/011)).

## Run tests

Single file:

```bash
aura test corpus/test/smoke.aura
# monorepo: cargo run -p aura-cli -- test corpus/test/smoke.aura
```

Whole package:

```bash
aura test corpus/multi
aura test examples/notes
```

## `@test` functions

Mark test entrypoints with `@test`. Use `assert` / `assert_eq` (and `std.assert` where applicable) for checks on `Int`, `String`, and `Bool` in the current MVP.

```aura
@test
fun adds() {
  assert_eq(1 + 1, 2)
}
```

See `corpus/test/` and package-level tests under `corpus/` / `examples/notes` for patterns that compile today.

## Filtering and reports

Select tests by substring with `--test-name` (or its alias `--filter`):

```bash
aura test corpus/test --test-name add
aura test corpus/test --filter add --format json
```

The JSON report contains package timing, pass/fail/skipped counts, per-test
status, and structured failure diagnostics. Tests that are not selected are
reported as `skipped`.

## Race detector

`aura race` runs the same test-shaped workflow with runtime race tracking
enabled. It accepts the same package, filter, JSON, and `--` process-argument
options:

```bash
aura race corpus/test --format json
```

## Design intent

RFC-011 describes the broader framework contract. The **working path today** is:
compile the package or file, discover `@test` functions, optionally filter them,
run them via the runtime, and report human or JSON results. Parallel workers,
JUnit/LCOV output, fixtures, and async test scheduling remain deferred.

## Next

- [Getting started](./getting-started.md)
- [Standard library](./standard-library.md)
- [RFC-011](/rfc/011) — testing framework design
