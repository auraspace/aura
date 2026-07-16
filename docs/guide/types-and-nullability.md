---
title: Types & nullability
section: Language
order: 32
summary: Scalars, non-null by default, T?, flow narrowing, and force-unwrap.
---

# Types & nullability

Normative rules: [RFC-002](/rfc/002). MVP keywords and surface: [RFC-001 §6.0](/rfc/001).

## Scalars you will see first

| Type | Notes |
| ---- | ----- |
| `Int` | Integer (overflow policy is documented in RFCs; prefer checked ops in dev) |
| `Bool` | `true` / `false` |
| `String` | Immutable string |
| `Array<T>` | Growable array — see [Arrays](./arrays.md) |

Function parameters and returns use explicit types in most examples:

```aura
fun add(a: Int, b: Int): Int {
  return a + b
}
```

## Non-null by default

- `T` means **must be present** — no implicit null.
- `T?` is the **opt-in nullable** form.

```aura
fun greet(name: String) {
  println(name)
}

fun maybeGreet(name: String?) {
  // name may be absent
}
```

This is a core product rule from [RFC-000](/rfc/000) / [RFC-002](/rfc/002): safety by default, escape hatches explicit.

## Flow narrowing

After a null check, the compiler treats the value as non-null on that path:

```aura
fun lenOrZero(s: String?): Int {
  if (s != null) {
    return s.len()
  }
  return 0
}
```

## Force-unwrap

`!!` asserts non-null. Prefer narrowing when you can; use `!!` when you have an invariant the type system does not see yet.

```aura
fun mustHave(s: String?): Int {
  return s!!.len()
}
```

Misuse can fail at runtime — treat it as an explicit escape hatch.

## Null coalesce

`?:` provides a default when the left side is null (supported in the current compiler path):

```aura
fun label(name: String?): String {
  return name ?: "anonymous"
}
```

## Generics (preview)

Type parameters monomorphize for concrete uses:

```aura
class Box<T>(var value: T) {}

fun id<T>(x: T): T {
  return x
}
```

Bounds (`T : Named`, `where`) are part of the type system design — see RFC-002 and corpus generics samples.

## Next

- [Classes, structs & interfaces](./classes-and-structs.md)
- [Arrays](./arrays.md)
- [RFC-002](/rfc/002)
