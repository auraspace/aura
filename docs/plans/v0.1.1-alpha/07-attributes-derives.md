# Workstream 07: Attributes and Derives

Owner: Compiler Expert. Scope: 6 tasks.

## M1. Attribute syntax

**Objective:** Parse attributes on every mandatory declaration site.
**Implementation status:** Complete for the current AST/parser surface. The
parser preserves ordered attributes, nested values, named/positional arguments,
and spans on declarations, members, fields, variants, and parameters.
**Checklist:**

- [x] Define argument forms, nesting, ordering, and source spans.
- [x] Reject malformed syntax with recovery suitable for diagnostics.
- [x] Preserve attributes across package boundaries.
      **Acceptance:** Valid sites parse and invalid forms produce stable diagnostics.
      **Verification:** Run syntax positives, malformed inputs, and span snapshots.
      **Dependencies:** B2, C1.

## M2. Attribute registry and diagnostics

**Objective:** Validate known attributes consistently.
**Implementation status:** Complete for the registered built-in attributes.
Unknown, misplaced, duplicate, conflicting, and invalid-argument cases emit
stable `AURA-M2-*` diagnostics with source spans.
**Checklist:**

- [x] Register name, allowed sites, arguments, retention, and conflicts.
- [x] Make unknown attributes hard errors according to the contract.
- [x] Assign stable error identity and source spans.
      **Acceptance:** Every invalid attribute explains the expected form and location.
      **Verification:** Run unknown, misplaced, duplicate, and conflicting cases.
      **Dependencies:** M1.

## M3. Metadata integration

**Objective:** Carry accepted metadata into tooling and code generation.
**Implementation status:** Complete for source-retained metadata. Accepted
attributes survive parser, semantic checking, and the checked AST consumed by
codegen/tooling; `@test` discovery is derived from the shared attribute list
while retaining the legacy boolean for compatibility.
**Checklist:**

- [x] Preserve metadata through checking and lowering.
- [x] Define compile-time versus emitted metadata retention.
- [x] Migrate test discovery to the shared attribute mechanism.
      **Acceptance:** Existing test discovery remains compatible and metadata is
      available to intended consumers.
      **Verification:** Run discovery regression and metadata golden tests.
      **Dependencies:** M2, B2.

## M4. Equals derive

**Objective:** Generate type-checked equality implementations.
**Implementation status:** Partial. `@derive(Equals)` and the legacy `Eq` alias
generate a checked `equals(other): Bool` method for class/struct declarations
whose fields are primitive, `String`, nullable primitive/String, or class
references. Duplicate methods and unsupported field types have stable
`AURA-M4-*` diagnostics. Nested value types, generic type parameters, and
enums remain deferred to the broader derive contract.
**Checklist:**

- [x] Support the implemented class/struct subset.
- [ ] Handle primitive, nested, generic, and nullable fields.
- [x] Respect visibility and unsupported-field diagnostics; generic visibility
  semantics remain deferred with nested/generic support.
      **Acceptance:** Generated members pass normal checking and code generation.
      **Verification:** Run positive/negative derive corpus and output checks.
      **Dependencies:** M3.

## M5. Hash derive

**Objective:** Generate deterministic hash implementations.
**Implementation status:** Partial. `@derive(HashCode)` (with the compatible
`Hash` alias) generates a public `hashCode(): Int` method for class/struct
declarations whose fields are non-null `Int` or `String`. The implementation
uses a fixed `17 * 31 + field.hash()` fold, preserving the derive span on the
synthetic member and field spans on `AURA-M5-*` diagnostics. Bool, class,
nested, generic, reference, and nullable fields are explicitly deferred.
**Checklist:**

- [x] Implement the deterministic non-null `Int`/`String` field subset and
      reject unsupported field types with stable diagnostics.
- [x] Define nullable, nested, and generic hashing behavior as diagnosed and
      deferred until the corresponding derive/type contracts are available.
- [x] Preserve derive source attribution on generated members and offending
      field attribution on diagnostics.
      **Acceptance (implemented subset):** Hash output is deterministic for the
      supported `Int`/`String` fields; full equality-field parity remains
      deferred until the broader field contracts are implemented.
      **Verification:** Focused sema invalid-field/duplicate tests and a
      codegen emission test pass. Collision, generic, and broader determinism
      corpus coverage remain deferred.
      **Dependencies:** M4.

## M6. Debug derive

**Objective:** Generate useful debug representations.
**Checklist:**

- [ ] Define field ordering and representation for supported types.
- [ ] Handle nested, nullable, generic, and unsupported fields.
- [ ] Attribute errors to the derive declaration and offending field.
      **Acceptance:** Generated debug output is deterministic and source-aware.
      **Verification:** Run runtime output, golden, and diagnostic fixtures.
      **Dependencies:** M3, M4.
