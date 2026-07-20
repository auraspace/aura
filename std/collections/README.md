# std.collections

Collections package (RFC-007).

**Status (C6f + C7a `get` + C7e `Set`):**

| API     | Notes                                                                                  |
| ------- | -------------------------------------------------------------------------------------- |
| `Map`   | String → Int, linear `put` / `get` (`Int?`) / `getOr` / `contains` / `len` / `isEmpty` |
| `map()` | Empty map factory                                                                      |
| `Set`   | String keys, linear `add` / `remove` / `contains` / `len` / `isEmpty`                  |
| `set()` | Empty set factory                                                                      |

**Not yet:** generic `Map<K,V>` / `Set<T>`, hash table, iteration on Map/Set.

**Also available language-wide:**

- Builtin `Array<T>`
- Duck / interface Iterable (`len` + `get(Int)`) for `for-in`
