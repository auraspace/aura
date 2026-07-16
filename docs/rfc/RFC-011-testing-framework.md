# RFC-011: Testing Framework

| Field        | Value                              |
| ------------ | ---------------------------------- |
| **RFC**      | 011                                |
| **Title**    | Testing Framework                  |
| **Status**   | Accepted                   |
| **Layer**    | Toolchain                          |
| **Authors**  |                                    |
| **Created**  | 2026-07-15                         |
| **Updated**  | 2026-07-16                 |
| **Estimate** | 20–40 pages                        |
| **Depends**  | RFC-001, RFC-007, RFC-009, RFC-012 |
| **Blocks**   | —                                  |

---

## 1. Abstract

This RFC specifies Aura’s **built-in testing** support: discovery via `@test`, assertions, async tests, unit vs integration layout, filtering, and coverage hooks. Tests run through `aura test` using the same compiler and runtime as production code.

**Toolchain today (2026-07-16):** `aura test` discovers `@test` functions; builtins `assert` / `assert_eq` (Int/String/Bool); `std.assert` package for package-mode asserts (C3d, C4h). No async tests, tags/filters, or coverage hooks yet.

## 2. Motivation

### 2.1 Problem statement

Testing bolted on via external-only frameworks fragments DX. First-party discovery and runner make TDD and CI default paths.

### 2.2 Why now

Language attributes (RFC-009) and CLI (RFC-012) need a concrete test model for Ship MVP.

### 2.3 Success metrics

| Metric    | Target                            |
| --------- | --------------------------------- |
| Discovery | Zero-config `@test` in package    |
| Async     | `async @test` works with runtime  |
| CI        | JUnit-like / JSON report optional |

## 3. Goals

- Attribute-based discovery.
- Rich asserts with good failure messages.
- Parallel test execution by default where safe.
- Integration test directory convention.
- Coverage hooks (LLVM/source-based).

## 4. Non-goals

- Full browser/e2e product testing.
- Mandatory mocking framework in core (ecosystem).
- Property-based testing in MVP (later).

## 5. Prior art & alternatives

| System         | Notes          | Take            |
| -------------- | -------------- | --------------- |
| Rust `#[test]` | Simple         | **Inspiration** |
| JUnit          | Ecosystem      | Report formats  |
| Go `testing`   | Table tests    | Style           |
| pytest         | Fixtures power | Later ecosystem |

## 6. Design

### 6.1 Overview

```text
@test functions → compile test harness → run in process(es) → report
```

### 6.2 Discovery

```aura
@test
fun adds() {
  assertEqual(1 + 1, 2)
}

@test
async fun fetches() {
  val v = await load()
  assertTrue(v > 0)
}
```

- Package-private `@test` functions are discovered (no need for `pub`).
- Ignore: `@test(ignore)` or `@ignore`.
- Tags: `@test(tag = "slow")` + `--tag` filter.

### 6.3 Layout

| Kind        | Location                                      |
| ----------- | --------------------------------------------- |
| Unit        | Same package, `*_test.aura` or inline         |
| Integration | `tests/*.aura` as separate crates linking lib |

Exact filename convention freeze later; document both.

### 6.4 Assertions (`std.test`)

```aura
assertTrue(cond)
assertEqual(actual, expected)
assertNotNull(x)
assertFails { throw ... }
```

- Failure captures file/line and pretty-prints values (`Debug` derive).

### 6.5 Runner

- `aura test` builds test targets and executes.
- Default: parallel by test function with isolation (process-per-test optional flag).
- Filter: `--test-name pattern`, file path args.
- Fail fast option.

### 6.6 Async & concurrency

- Async tests run on runtime scheduler.
- Timeouts per test configurable.
- Race detector can enable under `aura test --race` when available.

### 6.7 Coverage

- `--coverage` produces LCOV/HTML via instrumentation (LLVM).
- Not required for MVP exit; design hooks reserved.

### 6.8 Reports

- Human terminal default.
- `--format json` / junit xml for CI.

### 6.9 Examples

```text
aura test
aura test --release
aura test -- --tag slow
aura test packages/http
```

### 6.10 Error model / edge cases

| Case                     | Behavior            |
| ------------------------ | ------------------- |
| Compile fail in tests    | No run; show errors |
| Panic/uncaught exception | Test fail + stack   |
| Hang                     | Timeout fail        |
| Flaky                    | Retries not default |

### 6.11 Compatibility & migration

- Attribute names stable.
- Runner flags semver careful.

## 7. Open questions

| #   | Question                     | Options              | Owner | Status       |
| --- | ---------------------------- | -------------------- | ----- | ------------ |
| 1   | Inline vs `*_test.aura` only | both allowed         | Test  | **Resolved** |
| 2   | Process isolation default    | same-process default | Test  | **Resolved** |
| 3   | Fixture system               | later                | Test  | **Deferred** — post-MVP; no core fixture system yet |

## 8. Rationale & trade-offs

Built-in tests lower friction and unify CI. Keeping mocks out of core avoids framework wars. Parallel by default matches Go/Rust expectations; isolation trade-offs documented.

## 9. Unresolved / future work

- Benchmark harness (`@bench`)
- Snapshot testing
- Property-based testing library

## 10. Security & safety considerations

- Tests may execute untrusted project code—same trust as `aura run`.
- Do not network to registry during test unless deps resolve already.
- Coverage data should not embed secrets from env accidentally in reports.

## 11. Implementation plan (optional)

| Phase | Scope                   | Exit criteria    |
| ----- | ----------------------- | ---------------- |
| T0    | `@test` + asserts       | `aura test` pass |
| T1    | Async + filter          | CI sample        |
| T2    | Reports + coverage hook | JSON junit       |

## 12. References

- Rust test book; Go testing package
- RFC-009, RFC-012, RFC-007

---

## Changelog

| Date       | Author | Change                                       |
| ---------- | ------ | -------------------------------------------- |
| 2026-07-16 |        | Defer fixture system post-MVP |
| 2026-07-16 |        | Status → **Accepted** — Review: @test discovery + same-process MVP shipped and locked |
| 2026-07-16 |        | Note `aura test` + assert MVP shipped        |
| 2026-07-15 |        | Initial skeleton                             |
| 2026-07-15 |        | Solid draft: @test, runner, async            |
| 2026-07-15 |        | Lock discovery layout + same-process default |
