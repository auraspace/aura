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
**Checklist:**

- [ ] Preserve metadata through checking and lowering.
- [ ] Define compile-time versus emitted metadata retention.
- [ ] Migrate test discovery to the shared attribute mechanism.
      **Acceptance:** Existing test discovery remains compatible and metadata is
      available to intended consumers.
      **Verification:** Run discovery regression and metadata golden tests.
      **Dependencies:** M2, B2.

## M4. Equals derive

**Objective:** Generate type-checked equality implementations.
**Checklist:**

- [ ] Support the contract-defined class/struct/enum subset.
- [ ] Handle primitive, nested, generic, and nullable fields.
- [ ] Respect visibility, generics, and unsupported-field diagnostics.
      **Acceptance:** Generated members pass normal checking and code generation.
      **Verification:** Run positive/negative derive corpus and output checks.
      **Dependencies:** M3.

## M5. Hash derive

**Objective:** Generate deterministic hash implementations.
**Checklist:**

- [ ] Match equality field semantics and supported types.
- [ ] Define nullable, nested, and generic hashing behavior.
- [ ] Preserve source attribution for generated members.
      **Acceptance:** Hash output is stable and consistent with equality.
      **Verification:** Run collision, determinism, generic, and invalid-field cases.
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
