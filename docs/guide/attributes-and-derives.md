---
title: Attributes & derives
section: Language
order: 38
summary: Supported declaration attributes, test metadata, derives, and FFI annotations.
---

# Attributes & derives

Attributes use `@name(...)` syntax and are checked against the declaration
they annotate. Unknown attributes, invalid targets, duplicate non-repeatable
attributes, and conflicting attributes are compile-time errors.

## Tests and benchmarks

`@test` marks a zero-argument Aura function for `aura test`. Optional metadata
is supported:

```aura
@test(tag = "fast")
fun adds() {
  assert_eq(1 + 1, 2)
}

@test(ignore = true)
fun pending() {}
```

`@bench` is accepted as function metadata. Test functions cannot be generic,
take parameters, or be async in the current runner.

## Derives

`@derive` applies to a class or struct. The current implementations support
the following generated methods:

```aura
@derive(Equals, HashCode, Debug)
struct Point(val x: Int, val y: Int) {}
```

Supported names and generated surface:

| Derive                  | Generated method          |
| ----------------------- | ------------------------- |
| `Eq` / `Equals`         | `equals(other)`           |
| `Hash` / `HashCode`     | `hashCode()`              |
| `Debug` / `DebugString` | `toString()` / debug text |

Derives follow the type's field constraints. Unsupported field types produce
semantic diagnostics instead of silently generating partial methods.

## API and code-generation metadata

| Attribute                                                | Targets                   | Purpose                               |
| -------------------------------------------------------- | ------------------------- | ------------------------------------- |
| `@deprecated("message")` or `@deprecated(since = "...")` | declarations              | Mark an API as deprecated             |
| `@inline` / `@noinline` / `@cold`                        | functions and methods     | Backend optimization hints            |
| `@throws`                                                | functions and methods     | Declare exception metadata            |
| `@unsafe`                                                | types, functions, methods | Mark an unsafe boundary               |
| `@repr(Name)`                                            | types                     | Select a representation metadata name |
| `@reflect`                                               | types                     | Retain reflection metadata            |
| `@notNull`                                               | parameters                | Require a non-null parameter contract |

`@inline` and `@noinline` conflict when placed on the same declaration.
Metadata does not replace type checking or ownership checks.

## Foreign declarations

`@foreign` annotates an external function declaration and requires named ABI
metadata:

```aura
@foreign(library = "m", target = "native", link = "dynamic", abi = 1, abi_id = "c")
extern "C" fun native_abs(value: Int): Int
```

Supported metadata includes `library`, `target`, `link`, `abi`, `abi_id`, and
optional `failure`. Foreign declarations are an explicit runtime/codegen
boundary; they do not automatically make native resources safe to retain.

## Related guides

- [Testing](./testing.md)
- [Classes, structs & interfaces](./classes-and-structs.md)
- [Packages](./packages.md)
- [RFC-009](/rfc/009) for reflection metadata
