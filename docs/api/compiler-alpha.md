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
| `@derive(...)`                     | Supported subset | Expand the implemented built-in derives below              |
| `@deprecated(...)`                 | Reserved         | Emit a source diagnostic without changing runtime behavior |
| `@inline`, `@noinline`, `@cold`    | Reserved         | Optimization hints; ignored until backend support exists   |
| `@throws`, `@unsafe`, `@repr(...)` | Reserved         | Tooling/safety/layout metadata; no implicit behavior       |
| `@retention(...)`, `@attribute`    | Reserved         | Declare metadata retention and user attribute types        |
| `@reflect`, `@json`, `@notNull`    | Reserved         | Reflection and typed JSON mapping opt-ins                  |

Unknown attributes remain hard errors. Reserved attributes must produce an
explicit unsupported/reserved diagnostic until their phase is implemented.

## Derive names

The alpha derive vocabulary is:

- `Debug` -> generated `toString() -> String`
- `Equals` -> generated `equals(other) -> Bool`
- `Hash`/`HashCode` -> generated `hashCode() -> Int`
- `ToString` -> generated `toString() -> String`
- `Json` -> reserved mapping metadata; not implemented

The generated member spelling and collision rules are part of the contract.
The sema boundary records each generated item and its invocation span, and the
C backend exposes Binary/Runtime attribute metadata through
`AURA_METADATA_ABI_VERSION = 1`. Derives must never generate private ownership
bypasses or retain borrowed values across async boundaries.

`CheckedFile.attribute_metadata` retains normalized attribute names, arguments,
target, retention, and source span. `CheckedFile.expansions` records the
macro/derive phase, macro name, generated item, and both source spans. Source-retained
attributes remain available to compiler tools but are not emitted into the
binary metadata table. Expansion/attribute diagnostics carry a stable
`phase=derive` or `phase=attribute` marker, so tools can preserve expansion
ordering when presenting errors.

## Retention

The reserved levels are `Source`, `Binary`, and `Runtime`. The default for a
user attribute is `Binary`; `Runtime` requires explicit opt-in. Metadata format
versioning, package side tables, and runtime lookup remain compiler/runtime
work, not stdlib behavior.

## Blocking boundaries

- Parser/sema: attribute declarations, target validation, built-in derive
  expansion, collision diagnostics, and expansion-origin metadata.
- Codegen/runtime: versioned binary metadata emission, runtime retention,
  generic type identity, and generated ownership-safe members.
- Tooling: expansion previews, source maps, and stable diagnostics.

Compiler hosts can register a `UserDerive` implementation through
`check_file_with_derives`, or a deterministic AST `UserMacro` through
`check_file_with_macros`; both expand before typecheck and receive the same
ownership, diagnostics, and expansion-origin treatment as built-ins. The
lexer now provides a span-preserving, delimiter-aware token-tree model for
the next expansion phase. Source-level declarative matching and sandboxed
procedural derives remain a separate boundary: the language still needs
token-tree rule expansion and the RFC-010 out-of-process sandbox ABI before
arbitrary package code can execute during compilation.
