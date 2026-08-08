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

### Secondary constructors

Classes may declare additional overloads with `constructor`. Each one must
delegate to the primary constructor with `this(...)` before running its body:

```aura
class User(var name: String) {
  constructor(id: Int): this(id.toString()) {
    name = name + " (legacy)"
  }
}
```

Constructor calls select the primary or secondary overload by argument count
and type. Structs only support their primary constructor.

### Defaults, varargs, and overloads

Defaults are evaluated at the call site and must be trailing. Primary and
secondary constructors, class methods, interface methods, and top-level
functions may use them:

```aura
class User(val id: Int, val label: String = "user") {}
fun greet(prefix: String = "hello"): String { return prefix }
```

Use `vararg xs: T` for a final variadic parameter. Inside the declaration,
`xs` has type `Array<T>`; each call creates the array from the extra arguments:

```aura
fun count(vararg values: Int): Int { return values.len }
```

Overloads are selected by argument types, defaults, and generic bounds. An
ambiguous call is rejected with the candidate declaration spans.

## Inheritance and overriding

Aura supports **single class inheritance**. Classes are final by default; mark a
parent `open` before extending it. A subclass names its parent after `:` and
passes the parent constructor arguments in parentheses:

```aura
open class Animal(val age: Int) {
  open fun years(): Int {
    return this.age
  }
}

class Dog(val breed: Int) : Animal(7) {
  override fun years(): Int {
    return this.breed
  }
}

fun main() {
  val animal: Animal = Dog(42)
  println(animal.years().toString())
}
```

Rules for inheritance:

- A class can have at most one superclass; interfaces are listed after it.
- A final class cannot be extended. `open class` and `abstract class` can be
  used as parents.
- A method that replaces an inherited method must use `override`.
- The parent method must be `open` or `abstract`, and the override signature
  must match exactly.
- Calls through a parent-typed reference use the overridden method when the
  method is open.

Superclass constructor chaining is expressed in the class header, for example
`class Child(x: Int) : Parent(x)`. An overriding method may call the parent
implementation directly with `super.method(...)`.

See `corpus/class/inheritance.aura`, `corpus/class/virtual_dispatch.aura`,
`corpus/class/generic_inheritance.aura`, and
`corpus/class/string_constructor_ownership.aura`.

## Visibility and abstract classes

Members use package visibility by default. Add an explicit modifier when a
different boundary is needed:

| Modifier    | Visibility                                     |
| ----------- | ---------------------------------------------- |
| `pub`       | Accessible from other packages                 |
| _(default)_ | Accessible within the declaring package        |
| `private`   | Accessible only inside the declaring class     |
| `protected` | Accessible inside the class and its subclasses |

Visibility applies to constructor fields and methods:

```aura
open class Account(protected val id: Int, private val secret: Int) {
  pub fun accountId(): Int {
    return this.id
  }

  private fun checkSecret(value: Int): Bool {
    return value == this.secret
  }
}
```

`abstract class` declarations cannot be instantiated directly. Abstract
members participate in inheritance and override checks, and concrete
subclasses can override them. `open`, `final`, `abstract`, and `override` are
declaration modifiers, not runtime annotations.

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

Interfaces define method contracts. Classes implement them with a trailing
**`: Iface…`** after the primary constructor. A class may extend one class and
implement one or more interfaces:

```aura
interface Named {
  fun name(): String { return "default" }
}

class User(var id: Int) : Named {
  fun name(): String {
    return "user"
  }
}
```

Interface methods must be implemented with matching signatures unless they have
a default body. Interface dispatch is closed-world in the current C backend.
Structs cannot implement interfaces, and interfaces cannot be used as
superclasses.

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

Generic inheritance is supported when the concrete type arguments line up:

```aura
open class Parent<T>(val value: T) {
  open fun get(): T { return this.value }
}

class Child<T>(val childValue: T) : Parent<T>(childValue) {
  override fun get(): T { return this.childValue }
}
```

## Classes vs structs (practical)

| Prefer `class` when…          | Prefer `struct` when…            |
| ----------------------------- | -------------------------------- |
| Shared identity / heap object | Small value payload              |
| Interface polymorphism        | No need for implements           |
| Graph of objects              | Tight numeric or point-like data |

## OOP limits in the current MVP

- No multiple class inheritance.
- No struct inheritance or struct-to-interface implementation.
- Secondary constructors use explicit `constructor(...) : this(...)` delegation.
- Dispatch and generic specialization follow the closed-world C backend;
  LLVM currently accepts only the complete common MIR subset.

## Next

- [Types & nullability](./types-and-nullability.md)
- [Control flow & errors](./control-flow-and-errors.md)
- [Arrays](./arrays.md)
- [Syntax cheatsheet](./syntax-cheatsheet.md)
- [RFC-001](/rfc/001)
