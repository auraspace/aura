# C20h: `Array<Interface>` Layout Spike

**Date:** 2026-07-22  
**Status:** Deferred after bounded design spike  
**Scope:** Representation and ABI feasibility only; no compiler, runtime,
stdlib, corpus, CLI, roadmap, or debt changes.

## Question

Can the C backend represent an `Array<I>` with predictable element size,
interface dispatch, GC tracing, and drop behavior without introducing an
implicit ownership hole?

## Candidate layouts

### A. Inline fat pointers

Each element is a pair:

```c
typedef struct {
  void *data;
  const AuraIfaceVTable *vtable;
} AuraIfaceElem;
```

The Array buffer stores `AuraIfaceElem` values inline. `get` returns the pair,
and a method call dispatches through `vtable` with `data` as the receiver.

Advantages:

- One allocation for the Array buffer and no per-element box.
- Direct dispatch and good locality for the interface metadata.
- A stable element width once the pair ABI is frozen.

Costs and risks:

- Every Array copy/set/clear/drop operation must define retain/release for
  `data` and the vtable-associated ownership policy.
- GC must scan `data` according to the erased concrete type; a raw `void *`
  loses the information needed for uniform tracing unless the vtable carries
  mark/drop hooks.
- Nullable interfaces, value-backed implementations, and interface casts add
  sentinel and boxing rules to the ABI.
- Generated C needs specialized element helpers even though the buffer width
  is uniform.

### B. Boxed interface values

Each element stores one pointer to a GC-managed erased box:

```c
typedef struct AuraIfaceBox AuraIfaceBox;

typedef struct {
  void *payload;
  const AuraIfaceVTable *vtable;
  void (*drop)(void *payload);
  void (*mark)(void *payload);
} AuraIfaceBox;
```

The Array buffer is `AuraIfaceBox *`-sized. The box owns or explicitly
references the payload and centralizes dispatch, tracing, and finalization.

Advantages:

- Uniform pointer-sized Array elements and a simple generated-C layout.
- GC can treat the box as the root and use its `mark` hook for the payload.
- Drop behavior has one owner location, reducing type-erased double-free risk.
- A future implementation can support both class-backed and value-backed
  implementations by boxing the latter.

Costs and risks:

- Element insertion can allocate; iteration has an extra pointer indirection.
- Box retain/release semantics must be specified for Array copies, closure
  captures, and interface-to-interface casts.
- A box per element increases memory overhead and can increase GC work.
- The current generic pointer-box runtime primitive is not yet a complete
  erased-value ABI: it does not by itself establish concrete payload mark/drop
  contracts.

## Comparison

| Concern               | Inline fat pointer                | Boxed value                               |
| --------------------- | --------------------------------- | ----------------------------------------- |
| Array element width   | Two pointers                      | One pointer                               |
| Dispatch              | Direct                            | One extra indirection                     |
| Allocation            | Array only                        | Array plus element boxes                  |
| GC tracing            | Requires vtable mark metadata     | Box mark hook centralizes tracing         |
| Drop                  | Per-element erased hooks          | Box owns drop policy                      |
| Generated C           | Pair-specific helpers             | Pointer-sized generic helpers             |
| Value implementations | Need explicit boxing escape hatch | Natural fit                               |
| MVP risk              | High ABI and aliasing complexity  | Lower, but ownership still underspecified |

## Recommendation

**Defer implementation.** Neither candidate is ready for production because
Aura does not yet have borrow/lifetime rules for non-owning interface views or
a finalized erased-value ownership contract. Shipping the surface now would
make Array copy and drop semantics ambiguous.

When revisited, prototype the boxed layout first. It matches the current
GC-oriented runtime and keeps the C Array representation uniform. Before
implementation, require all of the following:

1. A documented retain/release rule for Array copy, set, clear, drop, and
   closure capture.
2. A vtable/box contract for `mark`, `drop`, nullability, and casts.
3. Generated-C smoke coverage for add, get, iteration, replacement, and GC
   teardown using both class-backed and value-backed implementations.
4. A decision on whether `Array<I>` owns boxes, borrows them, or supports both
   through distinct types.

This is a design result, not an implementation claim.
