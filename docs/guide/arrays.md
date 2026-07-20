---
title: Arrays
section: Language
order: 38
summary: Array<T> construction, len/get/set, push/pop, and for-in iteration.
---

# Arrays

Builtin `Array<T>` is the primary growable sequence type in the MVP ([RFC-001](/rfc/001)). Element types include `Int`, `Bool`, `String`, class references, structs, and enums (by value).

### Interface elements (C7h)

**MVP decision: reject `Array<I>` for interface `I`.** The monomorphized C layout needs a fixed element size; interface values are a closed-world tagged/fat layout that does not fit that model yet. The compiler emits a clear diagnostic (see `corpus/diag/array_interface.aura`).

Post-MVP options (not implemented): erase each element to a fat pointer `(dispatch tag, data*)`, or box every interface value as a heap object.

## Create and index

```aura
fun demo() {
  val xs = Array<Int>()
  xs.push(10)
  xs.push(20)
  val n = xs.len()
  val first = xs.get(0)
  xs.set(0, 11)
}
```

Out-of-bounds access is a runtime failure — prefer checking `len()` before `get`/`set` when input is untrusted.

## Grow and shrink

| Method       | Behavior                                       |
| ------------ | ---------------------------------------------- |
| `push(x)`    | Append; capacity grows as needed               |
| `pop()`      | Remove last; empty array throws                |
| `clear()`    | `len = 0`, keep capacity                       |
| `reserve(n)` | Ensure capacity (when available in your build) |

```aura
fun stack() {
  val xs = Array<Int>()
  xs.push(1)
  xs.push(2)
  val top = xs.pop()
  xs.clear()
}
```

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
  for (i in 0..xs.len()) {
    println(xs.get(i))
  }
}
```

String iteration over UTF-8 bytes as `Int` is also supported in the compiler path (`for (b in string)`).

## Arrays of class references

`Array` can hold class instances (heap references). Equality of class elements remains **identity** unless you compare fields explicitly.

## Ownership notes (implementation)

The C backend frees owned array buffers at scope end / before return in the current runtime model. Prefer clear lifetime patterns in local scopes; deeper GC interaction is evolving ([RFC-003](/rfc/003), [RFC-006](/rfc/006)).

**Element drop (C7j):** free is **buffer-only** — elements are not finalized. That is correct for primitives, class references (GC), and by-value enums/structs without owned buffers. Nested `Array<Array<T>>` deep free is deferred until nested mono emit order is fixed.

## Corpus

See `corpus/generic/array*.aura` and related control samples for executable truth.

## Next

- [Syntax cheatsheet](./syntax-cheatsheet.md)
- [Language tour](./language-tour.md)
