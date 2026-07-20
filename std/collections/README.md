# std.collections

Collections package (RFC-007).

**Status (C6f–C8d):**

| API           | Notes                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------- |
| `Map<K,V>`    | Linear `put` / `get` (`V?`) / `getOr` / `contains` / `remove` / `clear` / `len` / `isEmpty` |
| `map()`       | Empty `Map<String, Int>` factory                                                            |
| `Set`         | String keys, linear `add` / `remove` / `contains` / `len` / `isEmpty`                       |
| `set()`       | Empty set factory                                                                           |
| `Iterable<E>` | Protocol: `len(): Int` + `get(i: Int): E` for `for-in` (C8d)                                |

**Not yet:** generic `Set<T>`, hash table, iteration on Map/Set. Keys must support `==`.

**Also available language-wide:**

- Builtin `Array<T>`
- Duck Iterable (`len` + `get(Int)`) for `for-in` without implementing the interface
