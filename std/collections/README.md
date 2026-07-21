# std.collections

Collections package (RFC-007).

**Status (C6f–C18):**

| API                                | Notes                                                                                           |
| ---------------------------------- | ----------------------------------------------------------------------------------------------- |
| `Map<K,V>`                         | Linear `put` / `get` (`V?`) / `getOr` / `contains` / `remove` / `clear` / `len` / `isEmpty`     |
| `map_string_int()`                 | Empty `Map<String, Int>` factory (renamed when generic `map<T,R>` was added)                    |
| `Set<T>`                           | Linear `add` / `remove` / `contains` / `clear` / `len` / `isEmpty` / `get(i)` (C8g/C8h)         |
| `set()`                            | Empty `Set<String>` factory                                                                     |
| `Hashable`                         | `hash(): Int`; compiler-backed implementations for `Int` and `String` (C14)                     |
| `HashMap<K,V>`                     | Generic open addressing; `K: Hashable`; `hash_map()` and `hash_map_str()` factories (C14)       |
| `HashSet<T>`                       | Generic open addressing backed by `HashMap<T, Bool>`; `T: Hashable`; `hash_set()` factory (C15) |
| `keyArray()` / `valueArray()`      | Live `HashMap` snapshots in logical table order (C18)                                           |
| `toArray()`                        | Live `HashSet` snapshot in logical table order (C18)                                            |
| `map_hash_map_values`              | Generic `(K,V) -> R` map-entry HOF returning `Array<R>` (C18)                                   |
| `filter_hash_set` / `map_hash_set` | Generic set HOFs returning arrays (C18)                                                         |
| `Iterable<E>`                      | Protocol: `len(): Int` + `get(i: Int): E` for `for-in` (C8d)                                    |
| `map<T,R>`                         | `Array<T>` × `(T) -> R` → `Array<R>` (C16)                                                      |
| `filter<T>`                        | `Array<T>` × `(T) -> Bool` → `Array<T>` (C16)                                                   |
| `fold<T,A>`                        | `Array<T>` × init `A` × `(A, T) -> A` → `A` (C16)                                               |
| `map_ints`                         | Compatibility wrapper for `map<Int,Int>` (C10i)                                                 |
| `filter_ints`                      | Compatibility wrapper for `filter<Int>` (C10i)                                                  |
| `fold_ints`                        | Compatibility wrapper for `fold<Int,Int>` (C10i)                                                |
| `map_strings`                      | Compatibility wrapper for `map<String,String>` (C12o)                                           |
| `filter_strings`                   | Compatibility wrapper for `filter<String>` (C12o)                                               |
| `fold_strings`                     | Compatibility wrapper for `fold<String,String>` (C12o)                                          |
| `join`                             | `Array<String>` × sep → joined `String` (C12j)                                                  |

**Iteration:**

- `for (k in map.keys)` — keys field is `Array<K>`
- `for (x in set)` — duck Iterable via `len` + `get`
- `for (k in set.keys)` — same buffer via `Array` field

Generic `HashMap<K,V>` and `HashSet<T>` are monomorphized. Keys must satisfy `Hashable` and support `==`; the concrete factories remain compatibility sugar. Resize doubles capacity when load ≥ 1/2. Hash collection HOFs use free functions because Aura methods cannot declare their own type parameters yet; results are arrays in logical table order, excluding tombstones.

**Also available language-wide:**

- Builtin `Array<T>`
- Duck Iterable (`len` + `get(Int)`) for `for-in` without implementing the interface
