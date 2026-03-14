# Full OOP Support (Aura)

Aura is designed to support **full object-oriented programming (OOP)** with a TypeScript-like developer experience and a strict, compiler-friendly type system.

This document defines the intended OOP feature set and the semantics of the core constructs.

---

## OOP feature overview

- **Classes** with fields and methods
- **Constructors** with overload signatures
- **Single inheritance** via `extends`
- **Interfaces** and `implements`
- **Structural typing** for interfaces (duck-typing with static checks)
- **Access modifiers**: `public` (default), `protected`, `private`, `readonly`
- **Abstract classes** and **abstract methods**
- **Method overriding** with mandatory `override`
- **Static members** via `static`
- **Generics** on classes and methods
- **Overloads** for methods and constructors (multiple signatures + single implementation)

---

## Classes

### Declaring fields and methods

```typescript
class User {
  public name: string;          // `public` is the default
  private age: i32;
  readonly id: string;

  constructor(id: string, name: string, age: i32) {
    this.id = id;
    this.name = name;
    this.age = age;
  }

  function displayName(): string {
    return this.name;
  }
}
```

### `this`

Inside instance methods, `this` refers to the current object instance. Field access uses `this.fieldName`.

---

## Constructors and overloads

Aura supports **overload signatures** plus a single implementation body:

```typescript
class Point {
  x: f64;
  y: f64;

  constructor(x: f64, y: f64);
  constructor(xy: [f64, f64]);
  constructor(a: f64 | [f64, f64], b?: f64) {
    if (typeof a == "number") {
      this.x = a;
      this.y = b ?? 0.0;
    } else {
      this.x = a[0];
      this.y = a[1];
    }
  }
}
```

Rules (conceptual):

- Overload signatures appear before the implementation.
- The implementation may use union types and narrowing to support all signatures.

---

## Inheritance (`extends`) and `super`

Aura uses **single inheritance**:

```typescript
class Animal {
  protected name: string;
  constructor(name: string) { this.name = name; }
  function speak(): void { print("..."); }
}

class Dog extends Animal {
  constructor(name: string) { super(name); }
  override function speak(): void { print(this.name + " barks"); }
}
```

Intended semantics:

- A subclass instance is also an instance of its base class.
- `super(...)` invokes the base constructor and must run before accessing `this` (recommended rule).
- Overridden methods use **dynamic dispatch** (polymorphism) when called through a base-typed reference.

---

## Overriding (`override`)

Aura requires `override` when a method replaces a base-class method:

```typescript
class Base {
  function run(): void { print("base"); }
}

class Derived extends Base {
  override function run(): void { print("derived"); }
}
```

Why mandatory `override`:

- Prevents accidental overrides due to typos or signature drift
- Makes refactors safer and more obvious

---

## Abstract classes

Abstract classes are base types that cannot be instantiated directly. They can require subclasses to implement abstract members.

```typescript
abstract class Shape {
  abstract function area(): f64;
  function printArea(): void { print("area=" + this.area()); }
}

class Square extends Shape {
  size: f64;
  constructor(size: f64) { super(); this.size = size; }
  override function area(): f64 { return this.size * this.size; }
}
```

Rules (conceptual):

- Abstract methods have no body.
- Concrete subclasses must implement all inherited abstract members.

---

## Interfaces and structural typing

Interfaces are **contracts only** (no implementation). Aura uses **structural typing** for interface satisfaction:

```typescript
interface Named {
  name: string;
}

function greet(x: Named): void {
  print("Hello, " + x.name);
}

class Person {
  name: string;
  constructor(name: string) { this.name = name; }
}

greet(new Person("Ada")); // OK: Person structurally matches Named
```

`implements` is allowed for clarity/documentation and to force conformance checks, but structural typing means it is not the only way to satisfy an interface.

---

## Access modifiers

Aura supports:

- `public` (default): visible everywhere
- `protected`: visible in subclass chain
- `private`: visible only in the declaring class
- `readonly`: assignable only during construction (or initialization)

```typescript
class Account {
  public owner: string;
  protected balance: i64;
  private pinHash: string;
  readonly id: string;
}
```

Intent:

- Modifiers are enforced by the type checker (compile-time).
- The runtime representation may not require reflection or metadata for access checks.

---

## Static members

Static fields and methods belong to the class, not instances:

```typescript
class MathEx {
  static PI: f64 = 3.1415926535;
  static function sqr(x: f64): f64 { return x * x; }
}

print(MathEx.PI);
print(MathEx.sqr(3.0));
```

---

## Generics with OOP

Generics compose naturally with classes:

```typescript
class Box<T> {
  private value: T;
  constructor(value: T) { this.value = value; }
  function get(): T { return this.value; }
}
```

Constraints (conceptual):

```typescript
interface HasId { id: string; }

function getId<T extends HasId>(x: T): string {
  return x.id;
}
```

---

## Recommended OOP style (stdlib-friendly)

- Prefer **composition** over inheritance for shared behavior unless polymorphism is required.
- Keep base classes minimal; expose stable interfaces.
- Use `private`/`protected` to preserve invariants; expose readonly views where possible.
- Use unions + narrowing for “sum types” and model-level branching.

---

## Summary

Aura’s OOP model is intended to be:

- Familiar to TypeScript developers
- Strict and safe (no `any`, explicit `override`, strict nullability)
- Practical for real systems code (generics, access control, abstract types, deterministic semantics)

