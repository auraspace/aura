# RFC-009: Reflection & Metadata

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 009                       |
| **Title**    | Reflection & Metadata     |
| **Status**   | Accepted                   |
| **Layer**    | Language                  |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-07-16                 |
| **Estimate** | 30–50 pages               |
| **Depends**  | RFC-001, RFC-002          |
| **Blocks**   | RFC-004, RFC-010, RFC-007 |

---

## 1. Abstract

This RFC defines Aura’s **attributes (annotations)** and **metadata retention** model for compile-time tooling, derives, and **optional runtime reflection**. v1 favors **pay-as-you-go** metadata (not full Java-style always-on reflection) to keep single binaries lean while enabling serializers, test discovery, and DI-like libraries later—without putting frameworks in core.

## 2. Motivation

### 2.1 Problem statement

Derives, tests, and serializers need a uniform attribute system. Full runtime reflection on every type bloats binaries and hurts optimization; zero reflection blocks ecosystem patterns.

### 2.2 Why now

Macros (RFC-010), testing (RFC-011), and compiler emission depend on attribute grammar and retention.

### 2.3 Success metrics

| Metric           | Target                                  |
| ---------------- | --------------------------------------- |
| Attribute syntax | Stable, documented                      |
| Binary bloat     | No metadata for types without retention |
| Test discovery   | Works via attributes                    |
| Safe reflection  | Visibility respected                    |

## 3. Goals

- Unified attribute syntax on declarations and parameters.
- Retention policies: source / binary / runtime.
- Opt-in runtime type info (`TypeId`, members) for annotated types.
- Hooks for derive macros.

## 4. Non-goals

- Full dynamic `invoke` on every method of every class by default.
- Java classpath scanning of the world.
- Runtime code generation / JIT of new classes in v1.

## 5. Prior art & alternatives

| System                            | Notes           | Take              |
| --------------------------------- | --------------- | ----------------- |
| Java annotations + reflection     | Powerful, heavy | Opt-in retention  |
| C# attributes + reflection        | Similar         | Inspiration       |
| Rust attributes + limited reflect | Pay-as-you-go   | **Closer spirit** |
| Go tags + reflect                 | Struct tags     | Contrast          |

## 6. Design

### 6.1 Overview

```text
@attr(...)  on decl
    → compiler front records Attribute
    → expand derives (RFC-010)
    → emit metadata per retention
    → optional runtime APIs in std.reflect
```

### 6.2 Attribute syntax

```aura
@test
@derive(Equals, Hash)
@deprecated("use NewApi", since = "0.2")
@json(name = "user_id")
fun foo(@notNull x: String) { }
```

- Attributes are `@Name` or `@Name(args)`.
- Args: positional and/or named literals / simple consts.
- Repeatable attributes only if declared repeatable.
- **Unknown attributes are a hard compile error** (builtins + declared attribute types only).

### 6.3 Attribute declaration

```aura
@Attr(retention = Runtime, targets = [Class, Fun])
class JsonName(val name: String)
```

Or simpler v1: built-in set + user attributes as classes marked `@attribute`.

### 6.4 Retention

| Level     | Kept in                            | Use                |
| --------- | ---------------------------------- | ------------------ |
| `Source`  | Compiler session only              | Lints, local tools |
| `Binary`  | Package metadata / rlib side table | Link-time, tools   |
| `Runtime` | Embedded in binary                 | `std.reflect`      |

Default for user attrs: **Binary** (available to tools/dependents without runtime bloat). Runtime requires explicit opt-in.

### 6.5 Built-in attributes (non-exhaustive)

| Attribute                       | Role                                    |
| ------------------------------- | --------------------------------------- |
| `@test`, `@bench`               | Test discovery (RFC-011)                |
| `@derive(...)`                  | Macro derives (RFC-010)                 |
| `@deprecated`                   | Warnings                                |
| `@inline`, `@noinline`, `@cold` | Optimization hints                      |
| `@throws`                       | Doc/tooling only (unchecked exceptions) |
| `@unsafe`                       | Marks unsafe APIs                       |
| `@repr(...)`                    | Layout hints for structs/FFI            |
| `@retention` / meta-attributes  | On attribute types                      |

### 6.6 Runtime reflection API (sketch)

```aura
package std.reflect

class Type {
  fun name(): String
  fun methods(): Array<Method>
  fun fields(): Array<Field>
  // ...
}

fun typeOf<T>(): Type
fun typeIdOf<T>(): TypeId
```

Constraints:

- Only types with runtime metadata; others → error or limited info.
- Visibility: **public members only by default**; non-`pub` requires same-module or privileged API (if ever).
- Generics: may show raw class + type args if reified/recorded; mono instances may collapse—document honestly.

### 6.7 Interaction with monomorphization

- Concrete mono functions need not all appear in reflection.
- Class metadata describes the class template; type arguments recorded when retention requests it.

### 6.8 Examples

```aura
@derive(Debug)
@reflect  // opt into runtime metadata
class User(
  @json(name = "id") val id: Long,
  val email: String,
)
```

### 6.9 Error model / edge cases

| Case                     | Behavior                                              |
| ------------------------ | ----------------------------------------------------- |
| Unknown attribute        | Error or warn (feature-gated unknown attrs for tools) |
| Wrong target             | Error                                                 |
| Reflect without metadata | Clear runtime/compile error                           |
| Malformed attr args      | Compile error                                         |

### 6.10 Compatibility & migration

- Adding `@deprecated` is non-breaking.
- Changing retention of a public attribute type may be breaking for consumers.
- Metadata format versioned in binary.

## 7. Open questions

| #   | Question                                 | Options                     | Owner | Status                   |
| --- | ---------------------------------------- | --------------------------- | ----- | ------------------------ |
| 1   | Unknown attributes hard error?           | error / warn                | Lang  | **Resolved** — hard **error** on unknown attributes |
| 2   | Reflect private members?                 | no by default               | Lang  | **Resolved**             |
| 3   | Builtin `@reflect` vs retention on class | `@reflect` opt-in attribute | Lang  | **Resolved** (direction) |

## 8. Rationale & trade-offs

Pay-as-you-go metadata protects single-binary size and optimization. Attributes unify derives, tests, and future serializers. Limited reflection avoids building a second dynamic language on top of Aura. Cost: less “magic” than Java enterprise stacks—acceptable for core scope.

## 9. Unresolved / future work

- Full `std.reflect` surface
- Annotation processors beyond macros
- Source generators pipeline

## 10. Security & safety considerations

- Reflection must not silently pierce `private` across trust boundaries.
- Runtime metadata can leak class structure—document for security-sensitive apps; provide strip flags.
- Untrusted attributes from dependencies run only as compile-time macros under sandbox (RFC-010), not arbitrary runtime code.

## 11. Implementation plan (optional)

| Phase | Scope                    | Exit criteria |
| ----- | ------------------------ | ------------- |
| A0    | Parse + store attributes | Round-trip    |
| A1    | `@test` discovery        | `aura test`   |
| A2    | Opt-in runtime Type      | Demo inspect  |

## 12. References

- RFC-001, RFC-002, RFC-004, RFC-010, RFC-011
- Java retention policies; Rust attribute model

---

## Changelog

| Date       | Author | Change                                             |
| ---------- | ------ | -------------------------------------------------- |
| 2026-07-16 |        | Lock unknown attributes = error; Status → **Accepted** |
| 2026-07-16 |        | Status → **In Review** — Review: retention + opt-in reflect locked; unknown-attr still open |
| 2026-07-15 |        | Initial skeleton                                   |
| 2026-07-15 |        | Solid draft: retention, opt-in reflect             |
| 2026-07-15 |        | Lock Binary default retention; public-only reflect |
