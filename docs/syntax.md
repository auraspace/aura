# Aura Language Syntax Design (OOP, TypeScript‑like)

> **Purpose**: Define a clean, modern, and expressive syntax for the Aura programming language, inspired by TypeScript’s object‑oriented features while keeping the language lightweight and compiler‑friendly.

---

## Table of Contents

1. [Basic Types](#basic-types)
2. [Variables & Constants](#variables--constants)
3. [Operators](#operators)
4. [Control Flow](#control-flow)
5. [Arrays & Collections](#arrays--collections)
6. [Type System Deep Dive](#type-system-deep-dive)
   - [Nullability](#nullability)
   - [Tuples](#tuples)
   - [Type Alias](#type-alias)
   - [Union & Intersection](#union--intersection)
   - [Type Guards](#type-guards)
7. [Destructuring](#destructuring)
8. [Functions](#functions)
9. [Classes & Inheritance](#classes--inheritance)
10. [Interfaces & Structural Typing](#interfaces--structural-typing)
11. [Generics](#generics)
12. [Access Modifiers](#access-modifiers)
13. [Modules & Imports](#modules--imports)
14. [Enums & Literal Types](#enums--literal-types)
15. [Decorators](#decorators)
16. [Asynchronous Programming](#asynchronous-programming)
17. [Error Handling](#error-handling)
18. [Runtime & Memory Model](#runtime--memory-model)
19. [Style Guide & Best Practices](#style-guide--best-practices)

---

## Basic Types

| Type                       | Description              | Example                                 |
| -------------------------- | ------------------------ | --------------------------------------- |
| `bool`                     | Boolean value            | `let flag: bool = true;`                |
| `i32`, `i64`, `u32`, `u64` | Signed/unsigned integers | `let count: i32 = 42;`                  |
| `number`                   | Alias of `i32`           | `let n: number = 5;`                    |
| `f32`, `f64`               | Floating‑point numbers   | `let pi: f64 = 3.1415;`                 |
| `string`                   | UTF‑8 text               | `let name: string = "Aura"`             |
| `char`                     | Single Unicode scalar    | `let ch: char = '🦀';`                  |
| `void`                     | No value (for functions) | `function log(msg: string): void { … }` |

---

## Variables & Constants

- **Immutable binding** – `const` (compile‑time constant) or `let` (block‑scoped mutable).
- **Type annotation** is optional; if omitted, the compiler **infers** the type from the assigned expression.

```typescript
const MAX_USERS: i32 = 1000; // explicit type
let username = "alice"; // inferred as string
let answer = 42; // inferred as i32
```

---

## Operators

Aura supports standard arithmetic, logical, and comparison operators.

- **Arithmetic**: `+`, `-`, `*`, `/`, `%`, `**` (exponentiation).
- **Logical**: `&&`, `||`, `!`.
- **Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`.
- **Assignment**: `=`, `+=`, `-=`, etc.
- **Null-coalescing**: `??`.
- **Pipe Operator**: `|>` for chainable calls.

```typescript
let value = 10 |> add(5) |> multiply(2); // result: 30
```

---

## Control Flow

### If / Else

```typescript
if (count > 10) {
  console.log("Large");
} else if (count > 0) {
  console.log("Medium");
} else {
  console.log("Small");
}
```

### For Loops

```typescript
// Iterating over an array
for (const item of items) {
  console.log(item);
}

// C-style for loop
for (let i = 0; i < 10; i++) {
  console.log(i);
}
```

### Match (Pattern Matching)

Aura uses `match` for expressive branching, supporting exhaustive checking.

```typescript
match (status) {
    "idle" => console.log("Waiting..."),
    "loading" | "polling" => console.log("In progress"),
    "error" => {
        handleError();
        return;
    }
    _ => console.log("Unknown state") // catch-all
}
```

---

## Arrays & Collections

### Arrays

Arrays are fixed-type but dynamic-size collections.

```typescript
let list: i32[] = [1, 2, 3];
let names: Array<string> = ["Alice", "Bob"];

list.push(4);
const first = list[0];
```

### Maps and Sets

Using the built-in `Map` and `Set` classes.

```typescript
let map = new Map<string, i32>();
map.set("age", 30);

let set = new Set<string>(["a", "b", "c"]);
```

---

## Type System Deep Dive

### Nullability

Aura has **non‑nullable** types by default. To allow `null`, use the `?` suffix or a union with `null`.

```typescript
let name: string = "Aura";
// name = null; // Error

let middleName: string? = null; // OK
let email: string | null = null; // OK
```

### Tuples

Fixed-length arrays with different types for each element.

```typescript
let pair: [string, i32] = ["age", 25];
let [key, val] = pair; // destructuring
```

### Type Alias

```typescript
type ID = string | i32;
type Callback = (data: string) => void;
```

### Union & Intersection

```typescript
// Union: value can be one of several types
function printId(id: string | number) { ... }

// Intersection: combines multiple types
type Persistent = Serializable & Loggable;
```

### Type Guards

Use the `is` keyword for custom type guards that narrow types within a block. Since Aura has no top type (`any`/`unknown`), type guards are used with **Generics** or **Unions** to refine a known broad type into a specific one.

```typescript
function isString<T>(val: T | string): val is string {
  return typeof val == "string";
}

let x: number | string = "hello";
if (isString(x)) {
  console.log(x.length); // x is narrowed to string here
}
```

---

## Destructuring

Extract values from objects or arrays easily.

```typescript
// Array destructuring
const [x, y] = [10, 20];

// Object destructuring
const user = { name: "Alice", age: 30 };
const { name, age } = user;

// In parameters
function greet({ name }: User) {
  console.log(`Hello, ${name}`);
}
```

---

---

## Functions

All functions use the `function` keyword (instead of `fn`). Arrow‑style shorthand is optional but also uses `function` for consistency.

```typescript
function add(a: i32, b: i32): i32 {
  return a + b;
}

// Arrow‑style (still a function expression)
let mul = (x: i32, y: i32): i32 => x * y;
```

- Parameters are **covariant**; return types are **contravariant**.
- Supports **default parameters**, **rest parameters**, **function overloads**.

**Overload example**:

```typescript
function greet(name: string): string;
function greet(name: string, age: i32): string;
function greet(name: string, age?: i32): string {
  return age ? `${name} is ${age} years old` : `Hello, ${name}`;
}
```

---

## Classes & Inheritance

```typescript
class Animal {
    protected name: string;
    constructor(name: string) { this.name = name; }
    function speak(): void { /* … */ }
}

class Dog extends Animal {
    private breed: string;
    // Constructor overloads
    constructor(name: string);
    constructor(name: string, breed: string);
    constructor(name: string, breed?: string) {
        super(name);
        this.breed = breed ?? "unknown";
    }
    // Method override – must use `override` keyword
    override function speak(): void { console.log(`${this.name} barks!`); }
    // Overloaded method example
    function fetch(item: string): string;
    function fetch(item: string, times: i32): string;
    function fetch(item: string, times?: i32): string {
        const count = times ?? 1;
        return `${this.name} fetched ${item} ${count} time(s)`;
    }
}
```

### Abstract Classes

Abstract classes serve as base classes that cannot be instantiated directly and may contain abstract methods.

```typescript
abstract class Shape {
    abstract function area(): f64;
    function printArea(): void {
        console.log(`Area: ${this.area()}`);
    }
}

class Square extends Shape {
    size: f64;
    constructor(size: f64) { super(); this.size = size; }
    override function area(): f64 { return this.size * this.size; }
}
```

- **Single inheritance** with `extends`.
- **Abstract classes** via `abstract` keyword; can contain **abstract methods** that must be implemented by subclasses.
- **Mixins** can be expressed via `implements` multiple interfaces.
- **Static members** via `static` keyword.
- **Constructor overloads** and **method overloads** are allowed.
- **Method overriding** requires the `override` keyword for clarity.

---

## Interfaces & Structural Typing

```typescript
interface Shape {
    function area(): f64;
}

class Circle implements Shape {
    radius: f64;
    function area(): f64 { return Math.PI * this.radius * this.radius; }
}
```

- Interfaces are **pure contracts** – no implementation.
- Aura uses **structural typing**: any object with matching members satisfies the interface.

---

## Generics

```typescript
function identity<T>(value: T): T { return value; }

class Box<T> {
    private content: T;
    function new(item: T): Box<T> { this.content = item; return this; }
    function get(): T { return this.content; }
}
```

- Generic parameters can be constrained with `extends`.
- Supports **higher‑order generics** (e.g., `Fn<A, B>`).

---

## Access Modifiers

| Modifier    | Scope                                   |
| ----------- | --------------------------------------- |
| `public`    | Visible everywhere (default)            |
| `protected` | Visible to subclass chain               |
| `private`   | Visible only within the declaring class |
| `readonly`  | Immutable after construction            |

---

## Modules & Imports

```typescript
// file: math/util.ts
export function pow(base: f64, exp: i32): f64 {
  /* … */
}

// file: main.ts
import { pow } from "./math/util";
let result = pow(2.0, 8);
```

- Files are **modules**; top‑level `export` makes a symbol public.
- `import * as ns from "./mod"` for namespace import.
- Support **re‑export** (`export * from "./other"`).

---

## Enums & Literal Types

```typescript
enum Direction {
  North,
  East,
  South,
  West,
}
let dir: Direction = Direction.North;

// Literal union
type Status = "idle" | "running" | "error";
let s: Status = "idle";
```

- Enums are **numeric** by default, can be **string‑valued**.
- Literal types enable **exhaustive checking** in `match` statements.

---

## Decorators

Aura implements a type‑safe decorator system. Since the language does not provide an `any` type, decorators rely on **Generics** and **Built‑in Context Types** to inspect and modify behavior while maintaining strict type safety.

### Runtime Decorators

Runtime decorators are functions that wrap a class, method, or field. They use generics to preserve the signature of the target.

```typescript
// Generic Method Decorator
// T: The class type
// A: The argument types (tuple)
// R: The return type
function log<T, A, R>(
    target: (this: T, ...args: A) => R,
    context: MethodDecoratorContext<T, (this: T, ...args: A) => R>
) {
    const methodName = String(context.name);

    return function (this: T, ...args: A): R {
        console.log(`Entering method: ${methodName}`);
        const result = target.call(this, ...args);
        console.log(`Exiting method: ${methodName}`);
        return result;
    };
}

class UserService {
    @log
    function login(username: string): bool {
        // ...
        return true;
    }
}
```

- **MethodDecoratorContext** provides metadata such as the method's `name`, `private` status, and an `addInitializer` hook.
- The compiler ensures that the decorator's generic parameters (`T`, `A`, `R`) are correctly inferred from the decorated method.

### Build‑time Decorators (Attributes)

Build‑time decorators are used for metadata attachment or code generation. They run during the compilation phase and receive a **Metadata** object representing the AST node.

```typescript
// Define a build‑time attribute
@attribute
function Serializable(target: ClassMetadata) {
    target.addMetadata("serializable", true);
}

@Serializable
class User {
    name: string;
    age: i32;
}
```

- **@attribute** indicates that the function is a compiler plugin/macro.
- **ClassMetadata** provides access to the class's structure (fields, methods, types) at compile time.
- These are ideal for generating JSON serializers, ORM mappings, or enforcing architectural constraints.

---

## Asynchronous Programming

Aura supports asynchronous programming using the `async` and `await` keywords, built on top of the `Promise<T>` type.

### Async Functions & Promises

Any function marked `async` automatically returns a `Promise` of its return type.

```typescript
async function fetchUserData(id: string): Promise<User> {
  const response = await http.get(`/users/${id}`);
  return response.json<User>();
}

// Usage with .then()
fetchUserData("123").then((user) => {
  console.log(user.name);
});
```

### Await Expression

The `await` keyword can only be used inside `async` functions or at the top level of a module. It suspends execution until the promise is settled.

```typescript
async function main() {
  try {
    const user = await fetchUserData("456");
    console.log(`Hello, ${user.name}`);
  } catch (err: Error) {
    console.error("Failed to fetch user");
  }
}
```

### Concurrent Execution

Aura provides static methods on the `Promise` class for managing multiple concurrent operations.

```typescript
const [user, posts] = await Promise.all([
  fetchUserData(id),
  fetchUserPosts(id),
]);
```

- **Promise.all** waits for all to fulfill.
- **Promise.race** waits for the first to settle.
- **Promise.allSettled** waits for all to settle (either fulfilled or rejected).

---

## Error Handling

```typescript
try {
  // risky code
} catch (e: Error) {
  console.error(e.message);
} finally {
  // cleanup
}
```

- `Result<T, E>` type for functional error handling is also provided.

---

---

## Runtime & Memory Model

### Memory Management

Aura uses a high‑performance **Generational Garbage Collector** (GC). It is designed to minimize pause times and maximize throughput, making it suitable for both server‑side applications and interactive UI systems.

- **Stack Allocation**: Used for primitives and small local structs for speed.
- **Heap Allocation**: Used for classes, arrays, and long‑lived objects.

### Standard Library (Stdlib)

Aura's standard library is modular and provides essential primitives:

- `core`: Basic types, Exception, Promise.
- `fs`: File system operations.
- `io`: Streams, terminal I/O.
- `net`: HTTP and socket networking.
- `json`: Fast serialization and parsing.

```typescript
import { readFile } from "fs";
import { parse } from "json";

const content = await readFile("data.json");
const data = parse(content);
```

---

## Style Guide & Best Practices

- **Prefer `const`** over `let` when the binding does not change.
- **Explicit types** for public APIs; rely on inference for locals.
- **No `any` or `unknown`** – Aura is strictly typed with no "top type". Use **Generics** or **Union Types** to represent multiple possible types.
- **Keep classes small** – single responsibility.
- **Document generic constraints** for readability.
- **Use `readonly`** for immutable fields.
- **When overriding**, always use the `override` keyword.
- **When overloading**, declare all signatures before the implementation.

---

_This document serves as a living reference for the Aura language syntax. Future revisions will expand on advanced features such as pattern matching extensions and macro systems._
