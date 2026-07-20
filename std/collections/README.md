# std.collections

Collections package (RFC-007).

**Status (C6f–C8i):**

| API           | Notes                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------- |
| `Map<K,V>`    | Linear `put` / `get` (`V?`) / `getOr` / `contains` / `remove` / `clear` / `len` / `isEmpty` |
| `map()`       | Empty `Map<String, Int>` factory                                                            |
| `Set<T>`      | Linear `add` / `remove` / `contains` / `clear` / `len` / `isEmpty` / `get(i)` (C8g/C8h)     |
| `set()`       | Empty `Set<String>` factory                                                                 |
| `HashMap`     | String→Int open addressing; `hash_map()` capacity 16 (C8i)                                  |
| `Iterable<E>` | Protocol: `len(): Int` + `get(i: Int): E` for `for-in` (C8d)                                |

**Iteration:**

- `for (k in map.keys)` — keys field is `Array<K>`
- `for (x in set)` — duck Iterable via `len` + `get`
- `for (k in set.keys)` — same buffer via `Array` field

**Not yet:** generic HashMap, resize. Keys/elements must support `==`.

**Also available language-wide:**

- Builtin `Array<T>`
- Duck Iterable (`len` + `get(Int)`) for `for-in` without implementing the interface
