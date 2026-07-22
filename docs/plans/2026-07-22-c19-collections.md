# C19 — Collection access and iteration

## Objective

Extend `std.collections` after C18 with practical, stable collection access
APIs while preserving the current monomorphized representation and keeping
each change independently buildable.

## Working rules

- Agents run sequentially in the same worktree.
- Each agent owns one bounded slice and must create exactly one focused commit.
- Before committing, run the smallest relevant corpus checks plus affected Rust
  tests; do not amend or rewrite earlier commits.
- A later slice may build on an earlier commit, but must not mix unrelated debt.

## Dependency graph

```text
C19a safe lookup/accessors
          |
          v
C19x generic constructor substitution
          |
          v
C19y nested generic type substitution
          |
          v
C19b entry snapshot representation
          |
          v
C19c Iterable entry traversal + corpus
          |
          v
C19d docs, roadmap, debts, and full verification
```

## Task list

### C19a — Safe lookup/accessors

Add the smallest missing read-side APIs for `HashMap`/`HashSet` that can be
implemented using the existing table representation, with corpus coverage.
Do not introduce tuples, iterators, or compiler changes unless required by an
existing limitation discovered during implementation.

Acceptance: absent-key behavior is explicit and tested; APIs work for Int and
String keys; existing C18 behavior remains green.

Commit: `feat(collections): add hash collection accessors`

### C19b — Entry snapshots

Add a fixed-layout, generic entry snapshot API compatible with current Aura
value/array limitations. Prefer parallel key/value arrays or an existing
supported representation over introducing a new tuple ABI.

Acceptance: entries preserve key/value pairing and logical table order; source
collections are not mutated; Int and String corpus tests pass.

Commit: `feat(collections): add hash map entry snapshots`

### C19y — Nested generic type substitution prerequisite

Extend monomorphization/codegen so generic local, field, array-element, and
return types are substituted before C signatures and type keys are emitted.
This is the second prerequisite exposed by C19b; keep it general and do not
special-case `HashMapEntry`.

Acceptance: a generic function can return/use `Array<Generic<K,V>>` without
open `K`/`V` C symbols; existing generic class, interface, HOF, and collection
corpora remain green.

Commit: `fix(codegen): substitute nested generic types`

### C19x — Generic constructor substitution prerequisite

Fix the compiler/codegen path that emits a generic class constructor from a
generic function or method body. Type parameters such as `K` and `V` must be
substituted in the concrete monomorph before C symbols are emitted. This is a
prerequisite discovered while probing C19b; do not add collection-specific
workarounds.

Acceptance: a minimal generic function can construct and return a generic
class using its type parameters; existing generic class/interface and HOF
corpora remain green; no open `K`/`V` C identifiers remain in emitted code.

Commit: `fix(codegen): substitute generic class constructors`

### C19c — Iterable traversal

Make the useful new snapshot/accessor type participate in the existing
`Iterable<E>`/`for-in` protocol where the current type system can express it.
Keep unsupported interface-element layouts rejected with the existing
diagnostic.

Acceptance: end-to-end `for-in` corpus coverage; no regression in Array/Map/Set
iteration; compiler and runtime tests pass.

Commit: `feat(collections): support hash collection iteration`

### C19d — Close-out

Synchronize standard-library docs, corpus README, roadmap, and `agents/debts.md`
with the implemented behavior. Record any intentionally deferred limitation.

Acceptance: docs match the public API; `cargo test --workspace`, relevant
corpus checks, and `git diff --check` pass.

Commit: `docs(collections): close C19 collection batch`

## Checkpoints

- After C19a–C19b: compile and run all collection corpus packages.
- After C19x: run generic class/HOF and collection corpus packages before
  retrying C19b.
- After C19c: run the full workspace test suite and sanitizer smoke if practical.
- After C19d: review the complete commit series and verify a clean worktree.

## Risks

| Risk                             | Mitigation                                                       |
| -------------------------------- | ---------------------------------------------------------------- |
| Aura lacks tuple/entry value ABI | Use parallel arrays or an existing generic class representation. |
| Generic codegen regressions      | Keep Int/String corpus tests in every slice.                     |
| Iteration changes ownership      | Return snapshots and preserve current non-owning/view rules.     |
