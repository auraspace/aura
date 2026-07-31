# Aura Alpha Compiler Surface Lock

This file locks compiler-provided syntax and metadata names separately from
`std/` packages. A name can be reserved here before its expansion or runtime
backend exists; reserved names must fail with a stable diagnostic rather than
silently changing program meaning.

## Attributes

| Attribute                          | Alpha status     | Contract                                                   |
| ---------------------------------- | ---------------- | ---------------------------------------------------------- |
| `@test`                            | Supported        | Discover a package-private test function                   |
| `@foreign(...)`                    | Supported subset | Declare C library, target, link, ABI, and failure metadata |
| `@bench`                           | Reserved         | Discover a benchmark function; runner contract is deferred |
| `@derive(...)`                     | Reserved         | Expand only the locked derive names below                  |
| `@deprecated(...)`                 | Reserved         | Emit a source diagnostic without changing runtime behavior |
| `@inline`, `@noinline`, `@cold`    | Reserved         | Optimization hints; ignored until backend support exists   |
| `@throws`, `@unsafe`, `@repr(...)` | Reserved         | Tooling/safety/layout metadata; no implicit behavior       |
| `@retention(...)`, `@attribute`    | Reserved         | Declare metadata retention and user attribute types        |
| `@reflect`, `@json`, `@notNull`    | Reserved         | Reflection and typed JSON mapping opt-ins                  |

Unknown attributes remain hard errors. Reserved attributes must produce an
explicit unsupported/reserved diagnostic until their phase is implemented.

## Derive names

The alpha derive vocabulary is reserved as:

- `Debug` -> `debugString(value) -> String`
- `Equals` -> `equals(left, right) -> Bool`
- `Hash` -> `hash(value) -> Int`
- `ToString` -> `toString(value) -> String`
- `Json` -> `std.json.decode<T>` mapping metadata

The generated member spelling and collision rules are part of the contract;
the expansion backend is not yet implemented. Derives must never generate
private ownership bypasses or retain borrowed values across async boundaries.

## Retention

The reserved levels are `Source`, `Binary`, and `Runtime`. The default for a
user attribute is `Binary`; `Runtime` requires explicit opt-in. Metadata format
versioning, package side tables, and runtime lookup remain compiler/runtime
work, not stdlib behavior.

## Blocking boundaries

- Parser/sema: attribute declarations, target validation, derive expansion,
  recursion limits, and collision diagnostics.
- Codegen/runtime: binary metadata emission, runtime retention, generic type
  identity, and generated ownership-safe members.
- Tooling: expansion previews, source maps, and stable diagnostics.
