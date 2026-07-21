# C13p — Hashable / generic HashMap spike

| Field      | Value                                                                 |
| ---------- | --------------------------------------------------------------------- |
| **Opened** | 2026-07-21                                                            |
| **Slice**  | C13p (docs only)                                                      |
| **After**  | C8i `HashMap` String→Int; C9b resize; C12n `HashMapStr` String→String |
| **Goal**   | Design path to `HashMap<K,V>` without implementing it in C13          |

## Status

**Spike only.** No production `HashMap<K,V>`, no compiler protocol work in this slice.
Today’s stdlib remains two concrete open-addressing monos under `std/collections`.

## Why concrete `HashMap` / `HashMapStr` today

1. **Keys are fixed, hashing is pure Aura.** Both tables use the same `hash_string` (×31 over bytes) and open addressing with empty / full / tombstone `state`. That works without any language feature beyond `String` and `==`.
2. **Generics mono is real, but _bounded_ methods on type params are thin.** We can monomorphize `class Box<T>`, but a map that needs `hash(k)` + `k == k2` for arbitrary `K` needs either:
   - a **bound** (`K: Hashable`), or
   - **per-instantiation specialized copies** hand-written as separate classes (what we do now).
3. **Value type differs, algorithm does not.** `HashMap` vs `HashMapStr` is almost a copy-paste of `Array` element type for values. Duplication is intentional debt until one generic body can specialize.
4. **Array-of-interface is still out.** Erasing keys to a fat interface pointer would hit the same “fixed elem size / no fat-ptr array” MVP limit that rejects `Array<I>` (see debts / `corpus/diag/array_interface.aura`). Open addressing stores keys in `Array<K>`; that must stay monomorphized concrete layout for the current C backend.
5. **Product need was dogfood CLI maps**, not a full collections theory. String→Int (counts) and String→String (env-like tables) cover `examples/wc` and typical config maps.

**Bottom line:** concrete monos are the correct MVP. Generic form is blocked on a hash/eq protocol **and** monomorphized storage of `K`/`V`, not on “more copy-paste classes.”

## Current surface (reference)

| Type         | Keys                | Values   | Factory          | Notes                          |
| ------------ | ------------------- | -------- | ---------------- | ------------------------------ |
| `HashMap`    | `String`            | `Int`    | `hash_map()`     | C8i + C9b load ≥ 1/2 grow      |
| `HashMapStr` | `String`            | `String` | `hash_map_str()` | C12n; same probe/grow as above |
| `Map`        | any via linear scan | any      | `map()`          | Equality only; O(n)            |

Shared shape:

- fields: `keys: Array<K>`, `vals: Array<V>`, `state: Array<Int>`
- `put` / `get` / `remove` / `contains` / `grow` / `len` / `isEmpty` / `capacity`
- note in source: avoid `this.otherMethod()` chains (historical this-recv codegen bug); grow inlined into `put`

## Hashable protocol sketch

Prefer an **interface** bound (matches Iterable / existing iface mono) over magic compiler traits.

```aura
/// User-defined types that can be hash-map keys.
interface Hashable {
  /// Stable non-negative hash for table placement (or any Int; table abs/mod).
  fun hash(): Int

  /// Equivalence consistent with hash (a == b ⇒ a.hash() == b.hash()).
  /// Prefer language `==` where available; explicit method for user types.
  fun eq(other: Hashable): Bool
}
```

### Bound form for the map (target API)

```aura
// Sketch — not shipped
pub class HashMap<K, V> where K: Hashable (
  var keys: Array<K>,
  var vals: Array<V>,
  var state: Array<Int>,
) {
  pub fun put(k: K, v: V): Bool { /* probe using k.hash() + k.eq */ }
  pub fun get(k: K): V? { /* ... */ }
  // ...
}
```

If `where` bounds on class type params are awkward in today’s grammar, equivalent sketches:

```aura
// A) Bound on methods only (weaker; construction still unconstrained)
pub class HashMap<K, V>(...) {
  pub fun put(k: K, v: V): Bool where K: Hashable { ... }
}

// B) Separate key interface + adapter for builtins
interface HashableKey {
  fun hash(): Int
  fun eq(other: Self): Bool   // Self eq preferred if language grows it
}
```

### Builtin keys

| Type     | Strategy                                                                  |
| -------- | ------------------------------------------------------------------------- |
| `String` | Keep free `hash_string`; either auto-impl `Hashable` or special-case mono |
| `Int`    | `hash() = abs(self)` or mix; `eq` is `==`                                 |
| `Bool`   | 0/1                                                                       |
| classes  | User implements `Hashable`                                                |
| enums    | Derive later; manual `hash`/`eq` first                                    |

**Consistency rule (document in stdlib):** if `a.eq(b)` then `a.hash() == b.hash()`. Violation → lost entries, not a compiler error.

**Equality vs `==`:** for monomorphized `K` with structural/`==` support (String, Int), codegen can call `==` directly and only require `hash()`. For interface-typed keys, need `eq` on the vtable. Spike recommendation: **require `hash()` on the bound; use language `==` for mono `K` when available, `eq` only for erased path** (if erased path ever ships).

### Minimal std helpers (future, not C13)

```aura
fun hash_string(s: String): Int   // already private in collections; consider pub
fun hash_int(n: Int): Int
```

## Mono vs erase options

| Option                              | Idea                                                                     | Pros                                                              | Cons                                                                       | Fit for Aura now        |
| ----------------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------------------- | -------------------------------------------------------------------------- | ----------------------- |
| **M1. Monomorphize `HashMap<K,V>`** | One Aura generic class; compiler emits specialized C structs per `(K,V)` | Matches RFC-000/004 mono story; `Array<K>` stays fixed-size; fast | Needs bounds checking in sema; each pair grows code size                   | **Recommended**         |
| **M2. Keep hand monos**             | Add `HashMapInt`, `HashMapBoolStr`, … as needed                          | Zero language work                                                | Combinatorial explosion; maintenance clone of probe/grow                   | Acceptable interim only |
| **E1. Erase keys to `Hashable`**    | Store interface fat pointers in table                                    | One binary shape                                                  | **Blocked:** `Array` of interface rejected; GC/mark of iface elems; slower | Defer until Array\<I\>  |
| **E2. Type-id + void\* dictionary** | Runtime bag of untyped slots                                             | Very generic                                                      | Breaks Aura type story; unsafe; not v1                                     | Reject                  |
| **H1. Macro / codegen clone**       | Generate monos from template outside language                            | Avoids bounds                                                     | Tooling debt; not how std is written today                                 | Avoid                   |

**Recommendation:** **M1 monomorphization** for `HashMap<K,V> where K: Hashable` (or equivalent bound). Do **not** plan erase-based maps until `Array` of interface (or a dedicated fat-pointer array) exists.

### Monomorphization detail notes

- Instantiations: `HashMap<String, Int>` should ideally **alias or replace** today’s `HashMap` name; migration sketch in C14:
  - either rename current class → keep `HashMap` as generic and type-alias `HashMapStr` → `HashMap<String, String>`, or
  - introduce `HashMap2` / `Dict` briefly then rename at batch close.
- Prefer **one source** of probe/grow; delete duplicate `HashMapStr` body when generic lands.
- `V` has **no bound** for put/get; optional `V: Drop`/free hooks only if owned nested free becomes a language concern (out of scope; Array free is separate C13d debt).

## Recommended C14 next steps

Ordered so each step is shippable alone:

1. **Language/stdlib design lock (docs):** promote this spike into a short RFC-007 subsection or `docs/plans/c14-hashmap-generic.md` with final bound syntax chosen against then-current grammar.
2. **Sema: interface bound on class type param** (if not already usable for methods on `K`): `class HashMap<K: Hashable, V>` or `where K: Hashable` — whichever matches existing bound parsing.
3. **Stdlib: single generic open-addressing body** `HashMap<K, V>` with:
   - `k.hash()` for probe start
   - equality via `==` for known monomorphic builtins, else bound method
   - same load ≥ 1/2 grow / tombstones as C9b
4. **Migration:**
   - `type HashMapStr = HashMap<String, String>` (or keep name as wrapper factory)
   - keep `hash_map()` / `hash_map_str()` factories as sugar for a few releases
   - corpus: port `corpus/std_collections/hashmap*.aura` to generic form; add `HashMap<Int, String>` smoke
5. **Optional third concrete only if generic slips:** `HashMapInt` Int→Int for counters without String keys — **only** if a dogfood example needs it before M1 lands.
6. **Hashable for String/Int:** builtin impls or wrapper methods so user code is not forced to box primitives.
7. **Docs/guide:** collections page: when to use linear `Map` vs `HashMap`.

**C14 non-coupling:** do not block generic HashMap on async, LLVM, true borrow, or registry. It depends on **bounds + mono class fields** only.

## Explicit non-goals (C13 and near-term C14)

| Non-goal                                        | Why                                                       |
| ----------------------------------------------- | --------------------------------------------------------- |
| Full generic `HashMap<K,V>` in C13              | Slice is spike-only; C13 out of scope list                |
| Perfect hashing / Robin Hood / Swiss tables     | Open addressing is enough; perf later                     |
| Concurrent maps                                 | No shared-memory concurrency story yet                    |
| `HashSet<T>`                                    | Thin wrapper over map once generic exists; separate slice |
| Custom hasher injection / SipHash streaming API | Single process-local hash fine for std MVP                |
| Persistable / ordered maps                      | Different data structure                                  |
| Erased `HashMap` of interface keys              | Needs Array\<I\> (or equivalent)                          |
| Auto-derive `Hashable`                          | Manual impl first                                         |
| Changing `Map`/`Set` linear types in this spike | Leave as equality-only fallback                           |
| Compiler drive-by refactors for this note       | Docs only                                                 |

## Decision summary

| Question               | Answer                                                    |
| ---------------------- | --------------------------------------------------------- |
| Why two concrete maps? | Fixed String keys + value-type mono without bounds        |
| Protocol?              | `interface Hashable { fun hash(): Int; … }` + class bound |
| Mono or erase?         | **Mono** `HashMap<K,V>`; erase blocked on Array\<I\>      |
| C13 deliverable?       | This design note only                                     |
| C14 first code?        | Bound + one generic body; migrate HashMapStr → alias      |

## Related

- [C13 batch plan](./2026-07-21-next-20-c13a-c13t.md) (slice C13p)
- [C12 plan — HashMapStr](./2026-07-21-next-20-c12a-c12t.md)
- [std/collections README](../../std/collections/README.md)
- [agents/debts.md](../../agents/debts.md) — generic HashMap residual
- RFC-000 mono generics · RFC-002 types · RFC-007 stdlib · RFC-004 monomorphization
