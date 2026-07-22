# RFC-002: Type System

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 002                       |
| **Title**    | Type System               |
| **Status**   | Accepted                  |
| **Layer**    | Language                  |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-07-16                |
| **Estimate** | 40–60 pages               |
| **Depends**  | RFC-000, RFC-001          |
| **Blocks**   | RFC-004, RFC-007, RFC-009 |

---

## 1. Abstract

This RFC specifies Aura’s **static type system**: nominal class/interface types, nullability (`T` vs `T?`), generics with monomorphization, subtyping via inheritance and interface implementation, local type inference, overload resolution, flow-sensitive narrowing, and soundness goals.

It assumes the surface from **RFC-001** and leaves runtime representation details to **RFC-004** / **RFC-006**.

**Toolchain today (2026-07-22, C20e):** nominal classes/interfaces/`struct`/`enum`, monomorphized generics + bounds, local null flow + `!!` / `?:` / `?.`, type-argument inference, fun types/`Ty::Fun`, lambdas with value and reference captures, generic HOFs, nested generic substitution in codegen, and MVP shared mutable `var` captures for class/Array/Fun. Not yet: inheritance hierarchy, full overloading, structural typing, true borrow types, `Array<Interface>`, or a complete lifetime/ownership contract for captured Array views.

## 2. Motivation

### 2.1 Problem statement

A Java-like object model without modern nullability recreates classic NPE classes of bugs. A Go-like GC language without a strong static system shifts defects to production. Aura needs a **sound-enough, ergonomic** system that matches RFC-000 decisions.

### 2.2 Why now

Type rules gate compiler architecture, stdlib signatures, and reflection reification policy.

### 2.3 Success metrics

| Metric    | Target                                                                         |
| --------- | ------------------------------------------------------------------------------ |
| Soundness | No type confusion or null-on-`T` in safe code without explicit assert/`unsafe` |
| Inference | Most locals need no annotation; APIs remain explicitly typed at boundaries     |
| Generics  | Stdlib collections expressible; compile times acceptable via mono + caching    |
| Errors    | Actionable diagnostics with spans and expected/actual types                    |

## 3. Goals

- Static type checking as default for all Aura code.
- Expressive generics without unbounded type-level Turing tar pits (policy budget).
- Clear, teachable subtyping (nominal + interfaces).
- First-class nullability and `Result` integration.
- Coherent overloading and method resolution rules.

## 4. Non-goals

- Full dependent types.
- Gradual typing / `any` as a production default (may have `dyn` later—not v1).
- Higher-kinded types in v1.
- Structural typing as the default (optional structural records later).
- Ownership/borrow types (GC language).

## 5. Prior art & alternatives

| System         | Notes                       | Take                                              |
| -------------- | --------------------------- | ------------------------------------------------- |
| Java           | Nominal, erasure            | Nominal yes; **erasure no** for concrete generics |
| Kotlin         | Nullability, smart casts    | Adopt                                             |
| C#             | Generics reified-ish on CLR | Reification spirit via mono                       |
| TypeScript     | Structural, unsound holes   | Reject as default                                 |
| Rust traits    | Coherence, mono             | Interface dispatch + mono data                    |
| Hindley–Milner | Global inference            | Local/bidirectional only                          |

## 6. Design

### 6.1 Overview

Aura is **nominally typed** with a single inheritance hierarchy for classes and multiple interface implementation. Subtyping is explicit (extends/implements), not inferred from shape.

```text
Nothing  <:  all types          (bottom)
T        <:  T?
ClassS   <:  ClassT             if S extends T (transitively)
ClassC   <:  I                  if C implements I
```

`Any` (top for references) exists for rare heterogeneous APIs; prefer generics.

### 6.2 Type kinds

| Kind              | Description                                                      |
| ----------------- | ---------------------------------------------------------------- |
| Primitives        | `Int`, `Long`, `Float`, `Double`, `Bool`, `Char`, `Byte`, `Unit` |
| Class types       | User and stdlib classes; reference semantics                     |
| Interface types   | Existential-ish receivers; vtable dispatch                       |
| Enum types        | Closed variants; may be sugar over sealed class hierarchy        |
| Struct types      | Value types; no subclassing; copy semantics                      |
| Nullable          | `T?` = `T` ∪ `{ null }` with restrictions                        |
| Type constructors | `List<T>`, `Result<T, E>`, function types                        |
| `Nothing`         | Bottom; empty type (throw, infinite loop)                        |
| `Any`             | Top reference type (use sparingly)                               |

**Higher-kinded types:** not in v1.  
**Existentials / opaques:** `type Handle = opaque Long` or package-private class—lean simple opaque alias later.

### 6.3 Nullability & error types

#### Nullability

- `T` is **non-null** for reference types. Assigning `null` is a type error.
- `T?` accepts `T` or `null`.
- Dereference / method call on `T?` requires: safe call `?.`, explicit check + smart cast, `?:`, or `!!`.
- Primitives are non-null; use `Int?` for optional primitives (boxed/nullable representation).

**Lattice (simplified):** `Nothing <: T <: T? <: Any?` (details refined in formalization).

#### Errors

| Mechanism        | Use                                                 |
| ---------------- | --------------------------------------------------- |
| `Result<T, E>`   | Expected failure (parse, I/O not found, validation) |
| Exceptions       | Abnormal control flow; bugs; truly exceptional I/O  |
| `Nothing` return | `throw` expressions                                 |

Exceptions are **unchecked**: function types do not list throws. Documentation/`@throws` may exist for tooling but is not type-enforced in v1.

`Result` is a nominal generic sum type in prelude/stdlib:

```aura
enum Result<T, E> {
  case Ok(value: T)
  case Err(error: E)
}
```

### 6.4 Subtyping & assignability

**Assignability** `S` assignable to `T` when `S <: T` after nullability adjustment.

**Class subtyping:** single inheritance; transitive.  
**Interface subtyping:** if `I : J`, then `I <: J`. Implementing class is subtype of each interface.  
**Variance:**

| Position              | Default                                        |
| --------------------- | ---------------------------------------------- |
| Class type params     | **Invariant**                                  |
| Interface type params | Annotated `out` / `in` (Kotlin-style) allowed  |
| Function params       | Contravariant                                  |
| Function returns      | Covariant                                      |
| Arrays                | **Invariant** (no Java-style covariant arrays) |

**Boxing:** primitives convert to/from wrapper types only where defined (**minimal implicit**; prefer explicit conversions).

### 6.5 Generics

```aura
class Box<T>(var value: T)

interface Repo<T> where T : Entity {
  fun get(id: Id): T?
}

fun <T> identity(x: T): T = x
```

- **Bounds:** `T : ClassOrInterface` and multiple bounds `T : A, B`.
- **Where clauses** for complex constraints.
- **Associated types:** not v1 (use generic params).
- **Specialization:** not user-facing v1; compiler may specialize.
- **Const generics:** not v1.
- **Implementation:** **monomorphization** for concrete type args at codegen; interface-typed values use **vtable** / fat pointers as needed (RFC-004).

**Reification at runtime:** limited—see RFC-009. Prefer `TypeToken`/`reified` only if introduced later; v1 reflection may see erased interface views but mono preserves concrete layout in native code.

### 6.6 Type inference

- **Bidirectional checking:** check vs infer modes.
- **Locals:** infer from initializer.
- **Generics:** infer from arguments and expected type; explicit args when ambiguous.
- **Returns:** infer for expression-bodied locals; **public API** should state return types (style; may be enforced by lint).
- **Lambdas:** parameter types from expected functional interface / function type.
- **No global HM inference** across modules.

Ambiguity → hard error with candidate list (overloads/generics).

### 6.7 Interfaces & method resolution

- Interfaces may declare methods and default implementations.
- **Method resolution:** receiver static type → applicable members → most specific override; for interfaces, specificity rules + conflict error if two defaults collide without override.
- **Orphan rule (locked):** an `implements` / interface implementation for class `C` and interface `I` must live in the **same package as `C` or the same package as `I`**. No third-party packages may add implementations for foreign class + foreign interface pairs.
- **Marker interfaces:** empty interfaces allowed (`Sendable`-like names if needed—race model may not need them).

### 6.8 Flow-sensitive typing

After:

```aura
if (x is String) {
  // x : String
}
if (x != null) {
  // x : T when x was T?
}
```

Smart casts apply to **stable** locals (`val`) and limited `var` (not assigned in between). **Fields are not smart-cast in place**—copy to a local (`val t = this.field`) or use pattern matching.

**Exhaustiveness:** `match` on enums and sealed hierarchies must be complete or include `else`.

### 6.9 Type-level programming budget

Allowed: generics, bounds, associated constants **no**, type aliases, simple conditional types **no**.  
Forbidden in v1: arbitrary Turing-complete type computation, HKTs, full dependent types.

### 6.10 Interop with reflection (RFC-009)

- Runtime type tokens for classes that opt into metadata retention.
- Generic concrete mono instances need not reify all type args unless metadata requested.
- Casts checked against runtime class metadata when available; otherwise best-effort + unsafe.

### 6.11 Examples

```aura
fun len(s: String?): Int {
  if (s == null) return 0
  return s.length()  // s : String
}

fun <T : Comparable<T>> max(a: T, b: T): T {
  return if (a.compareTo(b) >= 0) a else b
}

fun parseInt(text: String): Result<Int, ParseError> {
  // ...
  return Result.ok(42)
}
```

### 6.12 Error model / edge cases

| Issue                         | Handling                                            |
| ----------------------------- | --------------------------------------------------- |
| Occurs check / infinite types | Reject                                              |
| Overload ambiguity            | Error with candidates                               |
| Inference explosion           | Complexity limits; ask for annotations              |
| Unchecked cast                | `as` may throw `CastError`; `as!` / unsafe bypasses |
| Raw types                     | None (no Java raw types)                            |

### 6.13 Compatibility & migration

- Adding a method to an interface is a breaking change unless defaulted.
- Generic variance annotations are part of public ABI of source.
- Edition may tighten inference or nullability edge cases.

## 7. Open questions

| #   | Question                                | Options                            | Owner | Status                                                                               |
| --- | --------------------------------------- | ---------------------------------- | ----- | ------------------------------------------------------------------------------------ |
| 1   | Orphan/impl coherence exact rule        | same package as class or interface | Lang  | **Resolved**                                                                         |
| 2   | Field smart-cast policy                 | locals only                        | Lang  | **Resolved**                                                                         |
| 3   | Primitive boxing implicit?              | minimal                            | Lang  | **Resolved**                                                                         |
| 4   | `Any` vs no top type                    | keep                               | Lang  | **Resolved** — keep `Any`                                                            |
| 5   | Sealed class syntax                     |                                    | Lang  | **Resolved** — `sealed class` / `sealed interface` (Kotlin-like); post-MVP implement |
| 6   | Variance annotation syntax (`out`/`in`) |                                    | Lang  | **Resolved** — `out`/`in` when introduced; default invariant until then              |

## 8. Rationale & trade-offs

Nominal typing fits Java-like classes and stable APIs. Nullability as types eliminates an entire defect class at modest annotation cost. Monomorphization favors native perf and simpler GC type layouts at the cost of code size—mitigated by LLVM and incremental builds. Rejecting HKTs and global inference keeps the compiler and mental model implementable for MVP. Unchecked exceptions + `Result` matches RFC-000 without Java’s checked-exception fatigue.

## 9. Unresolved / future work

- Formal soundness proof sketch
- Variance inference vs annotation-only
- Gradual/`dyn` story post-v1
- Reified generics for reflection ergonomics

## 10. Security & safety considerations

- Type confusion via bad casts is a security boundary; prefer checked `as`.
- `!!` and unsafe casts must be auditable.
- Reflection-based access must respect visibility (RFC-009) or require privilege.

## 11. Implementation plan (optional)

| Phase | Scope                      | Exit criteria            |
| ----- | -------------------------- | ------------------------ |
| T0    | Nominal core + nullability | Typecheck samples        |
| T1    | Generics + interfaces      | Stdlib types expressible |
| T2    | Inference polish           | DX benchmarks            |

## 12. References

- RFC-001, RFC-003, RFC-009
- TAPL (Pierce); Kotlin type system docs; Java generics (contrast erasure)

---

## Changelog

| Date       | Author | Change                                                                                           |
| ---------- | ------ | ------------------------------------------------------------------------------------------------ |
| 2026-07-16 |        | Lock sealed (`sealed class`/`interface`) + `out`/`in` variance direction                         |
| 2026-07-16 |        | Status → **Accepted** — Review: nominal/null/generics direction locked; sealed/variance deferred |
| 2026-07-16 |        | Note implemented type surface vs deferred                                                        |
| 2026-07-15 |        | Initial skeleton                                                                                 |
| 2026-07-15 |        | Solid draft: nominal, T?, Result, mono generics                                                  |
| 2026-07-15 |        | Lock orphan rule, smart-cast, Any, boxing                                                        |
