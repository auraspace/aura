# Union Types (Aura)

This document defines **union types** in Aura: what they mean, how to write them, and how to safely use them in a strictly typed language with **no `any` / `unknown`**.

---

## What is a union type?

A **union type** means “a value that is exactly one of several types”.

If a variable has type `A | B`, then at runtime it is either an `A` **or** a `B` (never both at the same time).

---

## Syntax

Union types use the `|` operator:

```typescript
type ID = string | i32;

let x: string | i32 = "abc";
x = 123;
```

### More than two members

```typescript
type Status = "idle" | "running" | "error";
```

### With `null` (nullable unions)

Aura is non-nullable by default. Add `null` via:

- The `?` suffix, or
- An explicit union with `null`

```typescript
let a: string? = null;           // sugar form
let b: string | null = null;     // explicit union form
```

`T?` is equivalent to `T | null`.

---

## Assignability rules (conceptual)

- A value of type `A` can be assigned to `A | B`.
- A value of type `B` can be assigned to `A | B`.
- A value of type `A | B` **cannot** be used where `A` is required unless the compiler can prove it is `A` (via narrowing).

```typescript
function takesString(s: string): void { /* ... */ }

let v: string | i32 = "hello";
// takesString(v); // Error: v might be i32
```

---

## Narrowing (how to use a union safely)

Aura must provide ways to **narrow** a union to a specific member type inside a control-flow region.

### 1) `typeof` checks (built-in)

```typescript
function printValue(v: string | i32): void {
  if (typeof v == "string") {
    // v is string here
    print(v.length);
  } else {
    // v is i32 here
    print(v + 1);
  }
}
```

### 2) Null checks

```typescript
function greet(name: string?): void {
  if (name == null) {
    print("Hello, stranger");
    return;
  }
  // name is string here
  print("Hello, " + name);
}
```

### 3) `match` (pattern-style narrowing)

For unions of literal types (and especially string literals), `match` is the most readable narrowing tool:

```typescript
type Status = "idle" | "running" | "error";

function render(s: Status): void {
  match (s) {
    "idle" => print("Waiting..."),
    "running" => print("Working..."),
    "error" => print("Something went wrong"),
    _ => print("Unknown"), // optional catch-all
  }
}
```

### 4) User-defined type guards (`is`)

Aura supports custom type guards via the `is` keyword. A type guard is a function that returns `bool` but also **proves** a refined type to the compiler inside an `if`:

```typescript
function isString<T>(val: T | string): val is string {
  return typeof val == "string";
}

function demo(x: i32 | string): void {
  if (isString(x)) {
    // x is string here
    print(x.length);
  } else {
    // x is i32 here
    print(x + 1);
  }
}
```

Guideline:

- Use type guards when a check is reused across the codebase, or when narrowing logic is more complex than a `typeof`/`null` check.

---

## Union types with classes and interfaces

Union members can be classes, interfaces, or structural types:

```typescript
interface Serializable {
  function toString(): string;
}

class User {
  name: string;
  constructor(name: string) { this.name = name; }
  function toString(): string { return this.name; }
}

class SystemError {
  message: string;
  constructor(message: string) { this.message = message; }
  function toString(): string { return this.message; }
}

type Loggable = User | SystemError;
```

To use member-specific APIs, you still need narrowing.

---

## Common patterns

### 1) “Either” return values (without `any`)

```typescript
function parseId(raw: string): string | i32 {
  // ... return string for UUID-like, i32 for numeric ...
}
```

Use narrowing at call sites to decide which branch you got.

### 2) Optional values (prefer `T?`)

If the only alternative is `null`, prefer `T?` for readability:

```typescript
function findUser(id: string): User? { /* ... */ }
```

---

## Interaction with intersection types (`&`)

Aura also supports intersection types (`A & B`) meaning “a value that satisfies both”.

Intersection and union can be combined, but require careful parentheses when the grammar needs it:

```typescript
type Persistent = Serializable & Loggable;
type MaybePersistent = (Serializable & Loggable) | null;
```

---

## Summary

- Use `A | B` to represent **one of several** types.
- Narrow unions using `typeof`, `== null`, `match`, or user-defined guards with `is`.
- Prefer `T?` for nullable values (\(T | null\)).

