---
title: Control flow & errors
section: Language
order: 36
summary: if/while/for, match on enums, Result, and throw/try/catch.
---

# Control flow & errors

## Branching

```aura
fun sign(n: Int): String {
  if (n > 0) {
    return "pos"
  } else if (n < 0) {
    return "neg"
  } else {
    return "zero"
  }
}
```

### `if` as expression (C4t)

When every branch ends in a value expression (and there is an `else`), `if` can produce a value:

```aura
fun label(x: Int): String {
  return if (x == 2) {
    "two"
  } else {
    "other"
  }
}
```

Corpus: `expr/if_expr.aura`.

### `is` in conditions

```aura
if (obj is Greeter) {
  println("greeter")
}
```

## Loops

```aura
// exclusive range
for (i in 0..3) {
  println(i)
}

// inclusive range
for (i in 0..=3) {
  println(i)
}

while (true) {
  break
}
```

`break` and `continue` work inside loops. Element iteration over arrays and string bytes is covered in [Arrays](./arrays.md).

## Enums and `match`

```aura
enum Color {
  case Red
  case Green
  case Blue
}

fun label(c: Color): String {
  match (c) {
    case Red => { return "red" }
    case Green => { return "green" }
    case Blue => { return "blue" }
  }
}
```

Arms should be **exhaustive** for the type being matched.

## `Result<T, E>`

Use `Result` for **expected** failures (parse errors, not-found, validation):

```aura
fun parseFlag(s: String): Result<Bool, String> {
  if (s == "true") {
    return Ok(true)
  }
  if (s == "false") {
    return Ok(false)
  }
  return Err("bad flag")
}
```

`std.io.Result<T, E>` uses `Ok(value)` and `Err(error)`. The shared
`std.error.Outcome<T, E>` uses `OutcomeOk(value)` and `OutcomeErr(error)` when
packages need a transport-neutral result surface. Both are ordinary enums and
can be handled with exhaustive `match`.

## Exceptions: `throw` / `try` / `catch` / `finally`

Use exceptions for **unexpected** failure paths (unchecked model per RFCs):

```aura
fun risky(flag: Bool): Int {
  if (flag) {
    throw "boom"
  }
  return 1
}

fun safe(): Int {
  try {
    return risky(true)
  } catch (e: String) {
    return 0
  } finally {
    // always runs
  }
}
```

Payload types currently include scalars and object-ish values in the implementation path — see compiler notes / corpus `control/try_catch.aura`.

**I/O note:** strict file APIs still throw `String` messages on failure, while
`std.io.readFileResult` / `writeFileResult` and the typed `std.error` wrappers
provide non-throwing alternatives. Text file reads are bounded at 256 MiB.

## Choosing Result vs throw

| Situation                               | Prefer   |
| --------------------------------------- | -------- |
| Caller is expected to handle it         | `Result` |
| Invariant broken / truly exceptional    | `throw`  |
| Library boundary with clear error codes | `Result` |

## Next

- [Arrays](./arrays.md)
- [Testing](./testing.md)
- [RFC-001](/rfc/001) · [RFC-002](/rfc/002)
