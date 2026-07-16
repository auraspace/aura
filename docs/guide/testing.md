---
title: Testing
section: Toolchain
order: 60
summary: Run @test functions with aura test and std.assert.
---

# Testing

Aura’s MVP test runner is the `test` CLI verb ([RFC-011](/rfc/011)).

## Run tests

Single file:

```bash
cargo run -p aura-cli -- test corpus/test/smoke.aura
```

Whole package:

```bash
cargo run -p aura-cli -- test corpus/multi
```

## `@test` functions

Mark test entrypoints with `@test`. Use `assert` / `assert_eq` (and `std.assert` where applicable) for checks on `Int`, `String`, and `Bool` in the current MVP.

```aura
@test
fun adds() {
  assert_eq(1 + 1, 2)
}
```

See `corpus/test/` and package-level tests under `corpus/` for patterns that compile today.

## Design intent

RFC-011 describes a broader framework (discovery, filtering, reporting). The **working path today** is: compile the package or file, discover `@test` functions, run them via the runtime, report pass/fail.

## Next

- [Getting started](./getting-started.md)
- [RFC-011](/rfc/011) — testing framework design
