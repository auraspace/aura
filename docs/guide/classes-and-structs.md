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

Structs are **values** (copy/by-value semantics at the model level). Primary constructor fields + methods; **no `implements`** in the current MVP.

```aura
struct Point(var x: Int, var y: Int) {
  fun translate(dx: Int, dy: Int) {
    this.x = this.x + dx
    this.y = this.y + dy
  }
}
```

Use structs when you want data without shared mutable identity.

## `interface` + `implements`

Interfaces define method contracts. Classes implement them; calls on interface-typed receivers use closed-world dispatch in the C backend.

**Generic interfaces (C7i):** the parser accepts `interface Iterable<E> { … }` and type-checks method signatures with those type parameters. **Implementing** a generic interface is not monomorphized yet — use a non-generic interface (fixed element type) for `for-in` protocols until implements type-args land.

```aura
interface Named {
  fun name(): String
}

class User(var id: Int) implements Named {
  fun name(): String {
    return "user"
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
| Interface polymorphism        | No need for `implements`         |
| Graph of objects              | Tight numeric or point-like data |

## Next

- [Control flow & errors](./control-flow-and-errors.md)
- [Types & nullability](./types-and-nullability.md)
- [RFC-001](/rfc/001)
