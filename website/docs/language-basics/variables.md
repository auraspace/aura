---
title: Variables & Mutability
sidebar_position: 1
---

# Variables & Mutability

In Aura, variables are used to store data that your program can manipulate. You can declare variables using the `let` and `const` keywords.

## `let` (Mutable Variables)

By default, variables declared with `let` are **mutable**. This means you can reassign their value after the initial declaration.

```aura
let x = 10;
print x; // 10

x = 20; // Reassignment is allowed
print x; // 20
```

## `const` (Immutable Constants)

Variables declared with `const` are **immutable**. Their value must be assigned at compile-time and cannot be changed during execution.

```aura
const PI = 3.14159;
PI = 3.14; // ❌ Compile Error: Cannot assign to constant PI
```

> [!TIP]
> Use `const` for values that are guaranteed never to change, such as configuration values or mathematical constants. Use `let` for everything else.

## Type Inference

Aura features strong type inference. While you can explicitly annotate types, the compiler can usually figure out the type based on the assigned value.

```aura
let name = "Aura"; // Inferred as string
let age: number = 5; // Explicitly annotated as number (i32)
```

## Naming Conventions

- Variable names must start with a letter or an underscore (`_`).
- Subsequent characters can be letters, numbers, or underscores.
- Aura uses `camelCase` for variables and `SCREAMING_SNAKE_CASE` for constants.
