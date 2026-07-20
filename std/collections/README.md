# std.collections

Collections package (RFC-007).

**Status (C6f + C7a `get`):**

| API     | Notes                                                                                  |
| ------- | -------------------------------------------------------------------------------------- |
| `Map`   | String → Int, linear `put` / `get` (`Int?`) / `getOr` / `contains` / `len` / `isEmpty` |
| `map()` | Empty map factory                                                                      |

**Not yet:** generic `Map<K,V>`, Set, hash table, iteration on Map.

**Also available language-wide:**

- Builtin `Array<T>`
- Duck / interface Iterable (`len` + `get(Int)`) for `for-in`
