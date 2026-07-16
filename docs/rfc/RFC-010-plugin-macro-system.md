# RFC-010: Plugin & Macro System

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 010                       |
| **Title**    | Plugin & Macro System     |
| **Status**   | Accepted                   |
| **Layer**    | Language                  |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-07-16                 |
| **Estimate** | 50–80 pages               |
| **Depends**  | RFC-001, RFC-009          |
| **Blocks**   | RFC-004, RFC-007, RFC-011 |

---

## 1. Abstract

This RFC defines Aura’s **macro and compiler plugin** story: **declarative macros** and **attribute derives** for MVP; **procedural macros / sandboxed plugins** (Rust-hosted or out-of-process) as a later phase. Goals are hygiene, predictable expansion order, and supply-chain safety—without requiring plugins to ship a basic language.

## 2. Motivation

### 2.1 Problem statement

Users need derives (`Equals`, `Debug`), test registration, and boilerplate reduction. Unrestricted in-process plugins are a supply-chain nightmare; no macros at all force painful codegen external to the language.

### 2.2 Why now

Compiler expansion points (RFC-004) and attributes (RFC-009) need a concrete macro model.

### 2.3 Success metrics

| Metric      | Target                                           |
| ----------- | ------------------------------------------------ |
| MVP derives | Equals/Hash/Debug work                           |
| Hygiene     | No accidental capture of user locals             |
| Safety      | Proc plugins cannot read arbitrary FS by default |

## 3. Goals

- Declarative macros + derive attributes in MVP.
- Hygienic expansion by default.
- Clear phase ordering with typecheck.
- Path to sandboxed procedural macros.

## 4. Non-goals

- Arbitrary compiler modification plugins in MVP.
- Unhygienic text-paste macros as the default.
- Full Template Haskell power day one.

## 5. Prior art & alternatives

| System      | Notes                 | Take                |
| ----------- | --------------------- | ------------------- |
| Rust macros | decl + proc, hygiene  | Primary inspiration |
| Lisp macros | Powerful              | Too free-form       |
| Java APT    | Build-time processors | Phase inspiration   |
| C macros    | Textual               | Reject              |

## 6. Design

### 6.1 Overview

| Feature                                    | MVP              | Later       |
| ------------------------------------------ | ---------------- | ----------- |
| Declarative `macro!` / `macro_rules`-style | Yes              | —           |
| `@derive(TraitLike)`                       | Yes              | —           |
| Attribute macros (custom)                  | Limited builtins | User proc   |
| Proc macros (Rust dylib / WASM sandbox)    | No               | Yes         |
| In-process arbitrary plugins               | No               | Maybe never |

### 6.2 Declarative macros

```aura
macro! vec {
  () => { Vec.new() };
  ($($x:expr),* $(,)?) => { /* ... */ };
}
```

- Pattern-based; expands to tokens/AST.
- Hygienic identifiers by default; explicit `span`/`unhygienic` opt-in rare.
- Invoked as `vec!(1, 2, 3)` or function-like form; exact grammar amended before implement (derives ship first).
- **Packaging:** macros live in normal packages (no separate macro package kind); consumers depend like any library.

### 6.3 Derive macros

```aura
@derive(Debug, Equals)
class Point(val x: Int, val y: Int)
```

- Built-in derives implemented in compiler or std plugins.
- Generate members: `equals`, `hashCode`, `toString`/`debugString`.
- User derives later via proc macros implementing a stable interface.

### 6.4 Expansion order

1. Parse AST.
2. Collect attributes & macro invocations.
3. Expand outer-to-inner with recursion limit.
4. Re-resolve names after expansion.
5. Typecheck (some derives may need partial types—phased expansion allowed for advanced derives later).

### 6.5 Procedural macros (phase 2)

- Implemented as **separate process** or **WASM sandbox** receiving serialized AST and returning generated items.
- Host language for authoring plugins: **Rust** first (matches toolchain).
- Capabilities: no network; FS limited to crate source root if needed; CPU/time limits.
- Distributed as versioned packages (RFC-005) with checksums.

### 6.6 Error model

| Case                    | Behavior                                    |
| ----------------------- | ------------------------------------------- |
| Expand error            | Span at invocation + macro definition notes |
| Infinite recursion      | Hard limit error                            |
| Plugin crash            | Compile error, not host ICE when sandboxed  |
| Unresolved after expand | Normal name resolution errors               |

### 6.7 Examples

```aura
@derive(Debug, Equals)
class User(val id: Long, val name: String)

fun demo() {
  val u = User(1, "a")
  println(debug(u))
}
```

### 6.8 Compatibility & migration

- Built-in derive names reserved.
- Proc macro ABI versioned; old plugins rejected with upgrade message.
- Editions may change hygiene edge cases carefully.

## 7. Open questions

| #   | Question                      | Options                | Owner     | Status       |
| --- | ----------------------------- | ---------------------- | --------- | ------------ |
| 1   | Declarative syntax exact form |                        | Lang      | **Resolved** — derives first; decl form = hygienic pattern macros (syntax amend pre-impl) |
| 2   | Proc sandbox: WASM vs process | separate process first | Toolchain | **Resolved** |
| 3   | Macro packaging unit          |                        | Pkg       | **Resolved** — normal packages export macros; no separate macro package kind |

## 8. Rationale & trade-offs

MVP without full proc macros reduces security and stability risk while covering 80% of boilerplate via derives and decl macros. Sandboxing later enables ecosystem power without making the compiler a plugin malware host. Cost: fewer magical frameworks early—aligned with core-only scope.

## 9. Unresolved / future work

- Full macro debugging tools (`cargo expand` equivalent)
- IDE expansion preview
- Official derive set list

## 10. Security & safety considerations

- Treat proc macros as untrusted code execution at compile time.
- Lockfile + checksums for plugin packages.
- Capability denial by default (network/FS).
- No ambient `unsafe` injection into user code without `unsafe` in expansion output still gated by user’s allowance—expanded `unsafe` should be visible (`macro expand` audit).

## 11. Implementation plan (optional)

| Phase | Scope              | Exit criteria      |
| ----- | ------------------ | ------------------ |
| P0    | Built-in derives   | Equals/Debug       |
| P1    | Declarative macros | vec!-like          |
| P2    | Sandboxed proc     | Custom derive demo |

## 12. References

- Rust book: macros; rustc expand
- RFC-004, RFC-009, RFC-005

---

## Changelog

| Date       | Author | Change                                         |
| ---------- | ------ | ---------------------------------------------- |
| 2026-07-16 |        | Lock derives-first + package-export macros; Status → **Accepted** |
| 2026-07-16 |        | Status → **In Review** — Review: derives MVP + process sandbox locked; declarative syntax open |
| 2026-07-15 |        | Initial skeleton                               |
| 2026-07-15 |        | Solid draft: derives MVP, sandboxed proc later |
| 2026-07-15 |        | Lock process sandbox for proc macros           |
