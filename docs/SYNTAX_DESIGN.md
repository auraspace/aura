# Aura Syntax Design (TS-like, Minimal OOP)

This document defines the initial, minimal syntax for Aura.

Aura aims for a TypeScript-like feel while remaining simpler to compile (ahead-of-time) into a single native binary.

Files:

- Primary extension: `.aura`
- Short extension: `.ar`

## Design Goals

- Familiar TypeScript-ish constructs: `class`, `interface`, `import`, `export`, `let`, `const`
- Statically typed with explicit types (limited inference in MVP)
- OOP-first: classes, `extends`, `implements`, `this`, `new`
- Simple control flow; avoid advanced TS features initially (union/intersection, structural typing, decorators)

## Lexical Structure

### Comments

- Line comment: `// ...`
- Block comment: `/* ... */`

### Identifiers

- ASCII letters, digits, and `_`, not starting with a digit.
- Keywords are reserved (see below).

### Literals

- Integers: `0`, `123`, `0xff`, `0b1010`
- Floats: `1.0`, `3.14`
- Booleans: `true`, `false`
- Strings: `"hello"` (UTF-8)

## Keywords (Initial)

```
class interface extends implements
function return
let const
if else while for break continue
try catch finally throw
import export
new this
true false
```

(More can be added later: `enum`, `abstract`, `static`, `public`, `private`, `protected`, `as`, etc.)

## Program Structure

### Modules

Aura uses file-based modules.

Imports:

```aura
import { Foo, bar } from "./foo"
import Baz from "./baz"
```

Exports:

```aura
export class Point { /* ... */ }
export function add(a: i32, b: i32): i32 { return a + b }
```

Notes (MVP):

- Import paths are relative and omit file extensions.
- Circular imports may be restricted initially.

## Types

### Built-in Types (MVP)

- Integers: `i32`, `i64`
- Floats: `f32`, `f64`
- Other: `bool`, `string`, `void`

`string` is a runtime-managed reference type.

### User Types

- `class` types (nominal)
- `interface` types (nominal in MVP)

### Type Annotations

Variables:

```aura
let count: i32 = 0
const name: string = "Aura"
```

Functions:

```aura
function inc(x: i32): i32 {
  return x + 1
}
```

## Variables and Assignment

- `let` declares a mutable binding
- `const` declares an immutable binding

```aura
let x: i32 = 1
x = x + 1

const y: i32 = 10
// y = 11  // error
```

## Functions

Syntax:

```aura
function name(param: Type, param2: Type): ReturnType {
  // ...
}
```

`void` functions omit a return value:

```aura
function log(msg: string): void {
  // runtime-provided printing in early milestones
}
```

## Classes (OOP Core)

### Class Declaration

```aura
class Point {
  x: f64
  y: f64

  function constructor(x: f64, y: f64): void {
    this.x = x
    this.y = y
  }

  function length(): f64 {
    return (this.x * this.x + this.y * this.y) // sqrt later
  }
}
```

Notes (MVP):

- Fields must be declared in the class body.
- The constructor is spelled `constructor` and is a normal instance method with special rules:
  - It must return `void`.
  - It may assign to `this.<field>`.

### Instantiation

```aura
let p: Point = new Point(1.0, 2.0)
```

### Inheritance

```aura
class Animal {
  function speak(): string { return "..." }
}

class Dog extends Animal {
  function speak(): string { return "woof" }
}
```

Dispatch rules (MVP, conceptual):

- Instance method calls are dynamically dispatched when the receiver static type is a base class or interface.
- The compiler may devirtualize when it proves the concrete type.

## Interfaces

```aura
interface Speaker {
  function speak(): string
}

class Robot implements Speaker {
  function speak(): string { return "beep" }
}
```

MVP semantics:

- Interfaces are **nominal**: `implements Speaker` is required.
- A cast syntax may be added later; for now, prefer explicit types.

## Expressions

### Operators (MVP)

- Arithmetic: `+ - * /`
- Comparison: `== != < <= > >=`
- Boolean: `&& || !`

Operator precedence should follow common C/TS conventions.

### Calls and Member Access

```aura
foo(1, 2)
obj.method(123)
obj.field
```

## Statements and Control Flow

### If / Else

```aura
if (x > 0) {
  return 1
} else {
  return 0
}
```

### While

```aura
let i: i32 = 0
while (i < 10) {
  i = i + 1
}
```

### For (Optional Sugar)

Aura may include a simple C-style `for` as syntax sugar:

```aura
for (let i: i32 = 0; i < 10; i = i + 1) {
  // ...
}
```

If implemented, the frontend lowers it to `while` in HIR/MIR.

## Error Handling (MVP)

MVP can start with:

- runtime panic via `panic("message")` (built-in function)
- language-level exceptions (`throw`, `try/catch/finally`) with simple, unchecked semantics

Later designs may introduce `Result<T, E>` and pattern matching.

### Exception Model (Initial)

Aura exceptions are objects (typically instances of `Error` or subclasses). Throwing an exception transfers control to the nearest surrounding `catch`. A `finally` block always executes, whether control exits normally, via `return`, or via `throw`.

#### Base Error Type

MVP standard library/runtime should provide:

```aura
class Error {
  message: string
  function constructor(message: string): void { this.message = message }
}
```

#### Throw

```aura
throw new Error("something went wrong")
```

#### Try / Catch / Finally

```aura
function readConfig(): string {
  try {
    return "ok"
  } catch (e: Error) {
    // handle or rethrow
    throw e
  } finally {
    // cleanup that must always run
    // (closing handles etc. can be added later)
  }
}
```

Notes (MVP):

- Exceptions are **unchecked** (no `throws` annotations required initially).
- `catch` binding type annotation is allowed and should be checked at runtime (via type id).
- Nested `try` blocks catch the nearest thrown exception first.
- In the MVP, exceptions do not cross foreign C boundaries; generated code must establish an Aura handler frame before invoking code that can `throw`.

## Standard Library Surface (Very Minimal)

To bootstrap programs, the compiler/runtime may provide a small set of built-ins:

- `function print(s: string): void`
- `function println(s: string): void`
- `function panic(s: string): never` (or `void` if `never` is not implemented yet)

These can be lowered to runtime ABI calls.

## Grammar Notes (Sketch)

This is not a full formal grammar, but a starting point for the parser:

- `Program := (ImportDecl | ExportDecl | TopLevelDecl)*`
- `TopLevelDecl := FunctionDecl | ClassDecl | InterfaceDecl`
- `Stmt := Block | LetDecl | ConstDecl | IfStmt | WhileStmt | ForStmt | ReturnStmt | ExprStmt`
- `Expr := Assignment | Binary | Unary | Call | Member | Primary`
