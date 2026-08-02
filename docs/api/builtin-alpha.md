# Aura Alpha Builtin API Lock

These names are compiler-provided and are always visible. Changes require a
language/compiler compatibility decision, not a casual stdlib edit.
The exact member signatures are mirrored in
[`builtin-signatures.tsv`](builtin-signatures.tsv) and checked by the API-lock
validator.

## Functions

| Name                                                        | Signature                                                                |
| ----------------------------------------------------------- | ------------------------------------------------------------------------ |
| `print`, `println`, `eprint`, `eprintln`                    | `(String) -> Unit`                                                       |
| `assert`                                                    | `(Bool) -> Unit`                                                         |
| `assert_eq`                                                 | `(Int, Int) -> Unit`, `(String, String) -> Unit`, `(Bool, Bool) -> Unit` |
| `gc_collect`                                                | `() -> Unit`                                                             |
| `exception_cause_count`                                     | `() -> Int`                                                              |
| `exception_source_span_start` / `exception_source_span_end` | `() -> Int`                                                              |
| `exception_cause_type`                                      | `(Int) -> String`                                                        |
| `exception_cause_span_start` / `exception_cause_span_end`   | `(Int) -> Int`                                                           |
| `exception_add_cause`                                       | `(String, Int, Int) -> Unit`                                             |

## Async language builtins

These are compiler syntax/type surfaces, not imported std functions:

| Surface                | Locked signature                                           |
| ---------------------- | ---------------------------------------------------------- |
| `async fun f(...): T`  | call expression -> `Task<T>`                               |
| `spawn { body -> T }`  | `TaskHandle<T>`; body is enqueued once                     |
| `await`                | `Task<T> -> T`; async context only                         |
| `join`                 | `TaskHandle<T> -> Result<T, TaskError>`; repeatable        |
| `cancel`               | `TaskHandle<T> -> Unit`; idempotent cooperative request    |
| `Channel<T>(capacity)` | `Int -> Channel<T>`; capacity must be positive             |
| `send`                 | `(Channel<T>, T) -> Unit`; FIFO queued payload             |
| `receive`              | `Channel<T> -> T?`; queued values then closed/null outcome |
| `close`                | `Channel<T> -> Unit`; idempotent                           |

Their ownership, cancellation, FIFO, and repeatable-join rules are locked by
RFC-003 and RFC-007; backend gaps remain implementation debt.

## Builtin types and methods

| Type       | Locked members                                                                                                                                                                                                                                                                                                                                                                                  |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Array<T>` | field `len`; `get(Int) -> T`; `set(Int, T) -> Unit`; `push(T) -> Unit`; `pop() -> T`; `clear() -> Unit`; `isEmpty() -> Bool`; `reserve(Int) -> Unit`; `clone() -> Array<T>`                                                                                                                                                                                                                     |
| `String`   | field `len`; `isEmpty() -> Bool`; `charAt(Int) -> Int`; `startsWith(String) -> Bool`; `contains(String) -> Bool`; `endsWith(String) -> Bool`; `hash() -> Int`; `indexOf(String) -> Int`; `split(String) -> Array<String>`; `trim() -> String`; `trimStart() -> String`; `trimEnd() -> String`; `toLower() -> String`; `toUpper() -> String`; `toInt() -> Int?`; `substring(Int, Int) -> String` |
| `Int`      | `toString() -> String`; `hash() -> Int`                                                                                                                                                                                                                                                                                                                                                         |

String indexing remains byte-oriented in alpha. Nullable-safe calls preserve
the existing `?.` behavior and return nullable values.
