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

`if` can also participate as an expression where the language surface allows (see RFC-001).

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
  Red,
  Green,
  Blue
}

fun label(c: Color): String {
  match (c) {
    Color.Red => { return "red" }
    Color.Green => { return "green" }
    Color.Blue => { return "blue" }
  }
}
```

Arms should be **exhaustive** for the type being matched.

## `Result<T, E>`

Use `Result` for **expected** failures (parse errors, not-found, validation):

```aura
fun parseFlag(s: String): Result<Bool, String> {
  if (s == "true") {
    return Result.Ok(true)
  }
  if (s == "false") {
    return Result.Ok(false)
  }
  return Result.Err("bad flag")
}
```

Exact variant spelling and helpers follow the compiler/corpus; treat this as the conceptual shape and check `corpus/enum/` samples.

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
