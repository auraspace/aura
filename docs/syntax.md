# Aura Language Syntax Guide

This document defines the syntax of the Aura programming language based on the current compiler implementation.

## Comments

Aura supports both single-line and multi-line comments.

- **Single-line comments**: `// content`
- **Multi-line comments**: `/* content */`
- **Documentation comments**:
    - Line: `/// content` (attaches to the following declaration)
    - Block: `/** content */` (attaches to the following declaration)

## Variables & Constants

Variables are declared using `let`, and constants are declared using `const`.

```aura
let x: i32 = 42;
const PI = 3.14; // Type inference supported
let name = "Aura";
```

## Types

Aura is a strictly typed language.

### Basic Types
- `i32`, `i64`: Integers
- `f32`, `f64`: Floating-point numbers
- `string`: UTF-8 strings
- `boolean`: `true` or `false`
- `void`: No return value

### Advanced Types
- **Unions**: `string | i32`
- **Arrays**: `i32[]`
- **Generics**: `List<string>`
- **Functions**: `function<T>(T): void`

## Expressions & Operators

### Arithmetic
`+`, `-`, `*`, `/`, `%`

### Comparison
`==`, `!=`, `<`, `<=`, `>`, `>=`

### Logical
`&&` (AND), `||` (OR), `!` (NOT - unary)

### Bitwise
`&` (AND), `|` (OR), `^` (XOR), `~` (NOT), `<<` (SHL), `>>` (SHR)

### Literal Expressions
- **Template Literals**: `` `Value: ${expr}` ``
- **Array Literals**: `[1, 2, 3]`
- **Null**: `null`

### Postfix Operators
- **Member Access**: `obj.member`
- **Index Access**: `arr[0]`
- **Function/Method Call**: `func(args)`

## Control Flow

### If Statement
```aura
if (condition) {
    // then branch
} else if (other) {
    // else if
} else {
    // else branch
}
```

### While Loop
```aura
while (condition) {
    // loop body
}
```

## Functions

Functions are first-class citizens in Aura.

```aura
function add(a: i32, b: i32): i32 {
    return a + b;
}

// Async functions
async function fetchData(url: string): Promise<string> {
    let response = await fetch(url);
    return response;
}
```

## Enums

```aura
enum Color {
    Red,
    Green,
    Blue = 10
}
```

## Object-Oriented Programming

### Classes
```aura
class Animal {
    private name: string;

    constructor(name: string) {
        this.name = name;
    }

    public function getName(): string {
        return this.name;
    }
}

class Dog extends Animal {
    override function getName(): string {
        return super.getName() + " (Dog)";
    }
}
```

- **Access Modifiers**: `public` (default), `private`, `protected`.
- **Members**: `static`, `readonly`, `abstract`, `override`.

### Interfaces
```aura
interface Shape {
    function getArea(): f64;
}

class Circle implements Shape {
    let radius: f64;
    function getArea(): f64 {
        return 3.14 * this.radius * this.radius;
    }
}
```

## Modules

Aura uses an explicit module system.

### Exporting
```aura
export let version = "1.0.0";
export function helper() {}
export class MyClass {}
```

### Importing
```aura
import { version, helper } from "./utils";
import * as utils from "./utils";
```

## Error Handling

```aura
try {
    throw new Error("Something went wrong");
} catch (e: Error) {
    print e.message;
} finally {
    // cleanup
}
```

## Special Operators
- **Type Test**: `expr is Type` (returns boolean)
