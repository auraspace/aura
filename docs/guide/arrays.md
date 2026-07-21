---
title: Arrays
section: Language
order: 38
summary: Array<T> construction, len/get/set, push/pop, and for-in iteration.
---

# Arrays

Builtin `Array<T>` is the primary growable sequence type in the MVP ([RFC-001](/rfc/001)). Element types include `Int`, `Bool`, `String`, class references, structs, enums (by value), and nested `Array<Array<T>>`.

### Interface elements (C7h)

**MVP decision: reject `Array<I>` for interface `I`.** The monomorphized C layout needs a fixed element size; interface values are a closed-world tagged/fat layout that does not fit that model yet. The compiler emits a clear diagnostic (see `corpus/diag/array_interface.aura`).

Post-MVP options (not implemented): erase each element to a fat pointer `(dispatch tag, data*)`, or box every interface value as a heap object.

## Create and index

```aura
fun demo() {
  val xs: Array<Int> = Array(0)   // capacity hint; grow with push
  xs.push(10)
  xs.push(20)
  val n = xs.len                  // field: element count
  val first = xs.get(0)
  xs.set(0, 11)
  if (xs.isEmpty()) {
    // …
  }
}
```

Out-of-bounds access is a runtime failure — prefer checking `len` before `get`/`set` when input is untrusted.

## Grow and shrink

| Method / field | Behavior                                            |
| -------------- | --------------------------------------------------- |
| `len`          | Element count (field)                               |
| `isEmpty()`    | `len == 0`                                          |
| `push(x)`      | Append; capacity grows as needed                    |
| `pop()`        | Remove last; empty array throws                     |
| `clone()`      | Owning buffer copy (C9c); nested Arrays deep-copied |
| `clear()`      | `len = 0`, keep capacity                            |
| `reserve(n)`   | Ensure capacity ≥ `n`                               |

```aura
fun stack() {
  val xs: Array<Int> = Array(0)
  xs.push(1)
  xs.push(2)
  val top = xs.pop()
  xs.clear()
}
```

## Higher-order helpers (C10i)

`std.collections` provides Int-specialized free functions that take first-class funs (see [syntax cheatsheet](./syntax-cheatsheet.md#lambdas-c10) for lambda forms):

| Helper        | Signature                                     |
| ------------- | --------------------------------------------- |
| `map_ints`    | `(Array<Int>, (Int) -> Int) -> Array<Int>`    |
| `filter_ints` | `(Array<Int>, (Int) -> Bool) -> Array<Int>`   |
| `fold_ints`   | `(Array<Int>, Int, (Int, Int) -> Int) -> Int` |

```aura
import std.collections

fun demo(xs: Array<Int>): Int {
  val doubled = map_ints(xs, (x: Int) => x * 2)
  // Array params own the buffer — clone when reusing the same Array.
  return fold_ints(doubled.clone(), 0, (a: Int, b: Int) => a + b)
}
```

Corpus: `fun/lambda_hof.aura`, `std_collections/hof`.

**Capture limits (C10h MVP):** lambdas may close over outer immutable `val` of `Int` / `Bool` / `String` only. Class, Array, `var`, and nested Fun captures are deferred ([debts](https://github.com/auraspace/aura/blob/main/agents/debts.md)).

Array parameters **own** the buffer (move at call site). Use `clone()` if you need the same array after a call that takes it.

## Iteration

```aura
fun sum(xs: Array<Int>): Int {
  var total = 0
  for (x in xs) {
    total = total + x
  }
  return total
}

fun indices(xs: Array<Int>) {
  for (i in 0..xs.len) {
    println(xs.get(i))
  }
}
```

String iteration over UTF-8 bytes as `Int` is also supported (`for (b in string)`). Duck / interface `Iterable` (`len` + `get`) works via `std.collections.Iterable<E>` and matching implementors.

## Arrays of class references

`Array` can hold class instances (heap references). Equality of class elements remains **identity** unless you compare fields explicitly.

## Ownership notes (implementation)

The C backend frees owned array buffers at scope end / before return / on owner reassignment. Prefer clear lifetime patterns in local scopes ([RFC-003](/rfc/003), [RFC-006](/rfc/006)).

**Element drop:**

- **Nested `Array<Array<T>>`:** deep-frees nested buffers on drop / clear / set (**C8e / C8f**).
- **Other elems:** buffer-only free is enough — primitives and by-value structs/enums need no dtor; class elems are GC roots.

## Corpus

See `corpus/generic/array*.aura` and related control samples for executable truth.

## Next

- [Syntax cheatsheet](./syntax-cheatsheet.md)
- [Language tour](./language-tour.md)
