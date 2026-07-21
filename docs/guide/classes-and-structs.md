---
title: Classes, structs & interfaces
section: Language
order: 34
summary: Reference classes, value structs, interfaces, and monomorphized generics.
---

# Classes, structs & interfaces

Normative object model: [RFC-001](/rfc/001), [RFC-002](/rfc/002), memory notes in [RFC-003](/rfc/003).

## `class` — reference types

Classes are **GC-managed references**. Primary constructor parameters become fields; methods use `this`.

```aura
class Counter(var n: Int) {
  fun inc() {
    this.n = this.n + 1
  }
}

fun main() {
  val c = Counter(0)
  c.inc()
  println(c.n)
}
```

### Defaults that matter

| Rule                  | Meaning                                               |
| --------------------- | ----------------------------------------------------- |
| **Final by default**  | Subclassing requires `open`                           |
| **Identity `==`**     | Class equality is reference identity (not structural) |
| **Nullable `Class?`** | Supported with correct heap emit + flow               |

See corpus under `corpus/class/` for working samples.

## `struct` — value types

Structs are **values** (copy/by-value semantics at the model level). Primary constructor fields + methods; **no interface implements** in the current MVP.

```aura
struct Point(var x: Int, var y: Int) {
  fun translate(dx: Int, dy: Int) {
    this.x = this.x + dx
    this.y = this.y + dy
  }
}
```

Use structs when you want data without shared mutable identity.

## `interface` + implements (`:`)

Interfaces define method contracts. Classes implement them with a trailing **`: Iface…`** after the primary constructor. Calls on interface-typed receivers use closed-world dispatch in the C backend.

```aura
interface Named {
  fun name(): String
}

class User(var id: Int) : Named {
  fun name(): String {
    return "user"
  }
}
```

### Generic interfaces (C8c / C9a)

Generic interfaces and **implements mono** ship in alpha:

```aura
interface Boxable<T> {
  fun get(): T
}

// Fixed type args on the implementor
class IntBox(val n: Int) : Boxable<Int> {
  fun get(): Int {
    return this.n
  }
}

// Generic class implements matching interface args
class Box<T>(val v: T) : Boxable<T> {
  fun get(): T {
    return this.v
  }
}
```

`std.collections.Iterable<E>` uses this path for `for-in` (see [Standard library](./standard-library.md)). Corpus: `iface/generic_impl.aura`, `iface/generic_class_impl.aura`.

### `is` type test (C9i)

```aura
fun check(n: Named) {
  if (n is User) {
    println("user")
  }
}
```

## Generics

- `class Box<T>`
- `fun id<T>(x: T): T`
- Inference from arguments / expected types (`Box("hi")`, `id(x)`)
- Bounds: `T : Named`, multi-bound `where`

Monomorphization produces specialized C symbols (e.g. `Box_String`).

## Classes vs structs (practical)

| Prefer `class` when…          | Prefer `struct` when…            |
| ----------------------------- | -------------------------------- |
| Shared identity / heap object | Small value payload              |
| Interface polymorphism        | No need for implements           |
| Graph of objects              | Tight numeric or point-like data |

## Next

- [Control flow & errors](./control-flow-and-errors.md)
- [Types & nullability](./types-and-nullability.md)
- [RFC-001](/rfc/001)
