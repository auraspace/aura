---
title: Functions & Scope
sidebar_position: 5
---

# Functions & Scope

Functions are the core units of logic in Aura, providing a way to encapsulate behavior and reuse code. Aura uses a clean, modern syntax for function declarations and supports full lexical scoping.

## Defining Functions

Functions are declared using the `function` keyword.

```aura
function add(a: number, b: number): number {
    return a + b;
}
```

- **Parameters**: Every parameter must have an explicit type annotation (`name: type`).
- **Return Type**: The return type is specified after a colon (`: type`).
- **Void Functions**: If a function does not return a value, the return type is `void` (optional).

```aura
function logMessage(message: string) {
    print message;
}
```

## Async Functions

Aura provides native support for asynchronous programming. Functions marked with `async` return a `Promise` and allow the use of the `await` keyword.

```aura
async function getUserData(id: string): User {
    let response = await api.fetch("/users/" + id);
    return response.json();
}
```

## Generic Functions

To create reusable code that works with different types, you can use Generics.

```aura
function wrapInArray<T>(item: T): T[] {
    return [item];
}

let numbers = wrapInArray<number>(42);
let strings = wrapInArray<string>("Aura");
```

## Lexical Scoping

Aura follows **Lexical Scoping** (also known as static scoping). The scope of a variable is determined by its location within the source code.

### Block Scope

Variables declared with `let` or `const` are scoped to the nearest enclosing block `{ ... }`.

```aura
let x = 10;
{
    let y = 20;
    print x; // Accessible (outer scope)
    print y; // Accessible (local scope)
}
print x; // OK
print y; // ❌ Error: y is not defined in this scope
```

### Nested Functions

Functions can be defined inside other functions, and they have access to the variables of the outer function.

```aura
function outer() {
    let secret = "Aura 2026";

    function inner() {
        print secret; // "Aura 2026"
    }

    inner();
}
```

### Class Scope

Inside class methods, instance members are accessed via the `this` keyword, ensuring clear separation from local variables.

```aura
class Counter {
    let count: number = 0;

    function increment() {
        this.count = this.count + 1;
    }
}
```

---

> [!NOTE]
> Functions in Aura are first-class residents, meaning they can be passed as arguments, returned from other functions, and assigned to variables.
