# std.collections

Collections package (RFC-007).

**Status (C6f–C14):**

| API              | Notes                                                                                           |
| ---------------- | ----------------------------------------------------------------------------------------------- |
| `Map<K,V>`       | Linear `put` / `get` (`V?`) / `getOr` / `contains` / `remove` / `clear` / `len` / `isEmpty`     |
| `map()`          | Empty `Map<String, Int>` factory                                                                |
| `Set<T>`         | Linear `add` / `remove` / `contains` / `clear` / `len` / `isEmpty` / `get(i)` (C8g/C8h)         |
| `set()`          | Empty `Set<String>` factory                                                                     |
| `Hashable`       | `hash(): Int`; compiler-backed implementations for `Int` and `String` (C14)                     |
| `HashMap<K,V>`   | Generic open addressing; `K: Hashable`; `hash_map()` and `hash_map_str()` factories (C14)       |
| `HashSet<T>`     | Generic open addressing backed by `HashMap<T, Bool>`; `T: Hashable`; `hash_set()` factory (C15) |
| `Iterable<E>`    | Protocol: `len(): Int` + `get(i: Int): E` for `for-in` (C8d)                                    |
| `map_ints`       | `Array<Int>` × `(Int) -> Int` → new array (C10i)                                                |
| `filter_ints`    | `Array<Int>` × `(Int) -> Bool` → new array (C10i)                                               |
| `fold_ints`      | `Array<Int>` × init × `(Int, Int) -> Int` (C10i)                                                |
| `map_strings`    | `Array<String>` × `(String) -> String` → new array (C12o)                                       |
| `filter_strings` | `Array<String>` × `(String) -> Bool` → new array (C12o)                                         |
| `fold_strings`   | `Array<String>` × init × `(String, String) -> String` (C12o)                                    |
| `join`           | `Array<String>` × sep → joined `String` (C12j)                                                  |

**Iteration:**

- `for (k in map.keys)` — keys field is `Array<K>`
- `for (x in set)` — duck Iterable via `len` + `get`
- `for (k in set.keys)` — same buffer via `Array` field

Generic `HashMap<K,V>` and `HashSet<T>` are monomorphized. Keys must satisfy `Hashable` and support `==`; the concrete factories remain compatibility sugar. Resize doubles capacity when load ≥ 1/2.

**Also available language-wide:**

- Builtin `Array<T>`
- Duck Iterable (`len` + `get(Int)`) for `for-in` without implementing the interface
