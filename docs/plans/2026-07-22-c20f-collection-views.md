# C20f — Collection iterator and entry-view contract

## Objective

Define the public contract for collection traversal before implementing live
iterators or mutable entry views. This task is design-only: it must not alter
stdlib code, compiler behavior, runtime ABI, corpus fixtures, CLI behavior,
roadmap, debt tracking, or the C20–C21 parent plan.

## Decision summary

Snapshot traversal is the only stable/default behavior in this wave. Live
iterators and entry views are future opt-in APIs whose semantics must be
specified before implementation.

| Concern | Contract for snapshots | Contract required before live implementation |
| --- | --- | --- |
| Source mutation | Snapshot is unaffected by insert, remove, clear, or rehash | Each mutation class needs explicit visibility rules |
| Order | Logical order at creation; later source order is irrelevant | Position and insertion visibility must be documented |
| Invalidation | None from source mutation | Detect invalidation; return typed error or terminal state |
| Entry lifetime | Owned copy; may escape freely | Bounded by source and cursor validity epoch |
| Aliasing | No alias to source storage | No structural mutation while entry is mutably borrowed |
| GC/ownership | Snapshot values follow normal value/GC rules | View retains source; entry values remain GC-visible; no raw storage ownership |
| `for-in` | Uses snapshot traversal | Live traversal requires a distinct named API |

## Scope

In scope:

- Snapshot versus live traversal semantics.
- Mutation visibility and invalidation policy.
- Entry lifetime and escape restrictions.
- Aliasing and mutation-through-entry rules.
- Source retention, element reachability, and GC behavior.
- Acceptance criteria for a future implementation wave.

Out of scope:

- Aura or Rust implementation.
- New runtime functions or ABI changes.
- Borrow/reference syntax or lifetime checking.
- Corpus fixtures and CLI flags.
- Roadmap, technical-debt, or parent-plan updates.

## Contract details

### Snapshot traversal

Snapshot constructors copy the logical sequence. A snapshot remains valid
after source insertion, removal, clear, and hash-table rehash. Its order is
the source's documented logical order at creation; it does not track later
source ordering. Snapshot entries contain copied key/value data and may be
returned, stored, or iterated after the source is dropped.

### Live traversal

Live traversal is not part of the current default API. A future constructor
must state which post-creation mutations are visible before implementation.
Rehash, remove, clear, and capacity-changing insertion invalidate a cursor by
default. Stale cursors must produce an explicit invalidation result or become
terminal; they must never access stale buckets or backing-array slots.

### Entry views

A live entry is a borrowed handle, not an owned pair. It cannot outlive the
source collection or the cursor validity epoch. Keys are read-only. Value
assignment requires an explicitly mutable entry API. Removal invalidates the
entry. Structural mutation while an entry is borrowed requires exclusive
access or must return invalidation; it cannot silently create an aliasing
hazard.

### GC and ownership

A live view retains its source for the handle's lifetime. Values reachable
through a view or entry remain GC-visible. Dropping a view releases only its
retention and never frees storage still owned by the source. Public APIs must
not expose raw bucket or backing-array pointers.

## Implementation gate

The implementation wave may start only when its API documentation includes:

1. order and mutation visibility for every mutation class;
2. invalidation result/state and cursor behavior;
3. source, iterator, entry, and element lifetimes;
4. aliasing rules for reads, value assignment, and removal;
5. GC roots and ownership transfer;
6. corpus tests for mutation, rehash, clear, entry escape, and reclamation.

## Commit

`docs(c20): define collection view contract`
