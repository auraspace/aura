---
title: Data Types
sidebar_position: 2
---

# Data Types

Aura is a strongly and statically typed language. Every value has a specific type that is checked at compile-time.

## Primitive Types

Aura provides several built-in primitive types:

| Type | Description | Example |
| :--- | :--- | :--- |
| `number` | 32-bit signed integer (i32) | `42` |
| `i64` | 64-bit signed integer | `10000000000L` |
| `f32` | 32-bit floating point | `3.14f` |
| `float` | 64-bit floating point (f64) | `2.718` |
| `string` | UTF-8 encoded string | `"Hello Aura"` |
| `boolean`| Boolean value | `true` or `false` |
| `null`   | Represents the absence of a value| `null` |
| `void`   | Represents no value (functions only)| `void` |

## Compound Types

### Arrays
Arrays are ordered collections of values of the same type. They are declared using square brackets `[]`.

```aura
let scores: number[] = [95, 88, 72];
let firstScore = scores[0]; // 95
```

### Union Types
Union types allow a variable to hold values of multiple different types.

```aura
let id: string | number = 101;
id = "U-102"; // OK
```

### Object Types
Object types define the structure of anonymous objects.

```aura
let user: { name: string, active: boolean } = {
    name: "Alice",
    active: true
};
```

## Type Testing (`is`)

You can check if a value is of a certain type using the `is` operator. This is particularly useful when working with Union types.

```aura
let val: string | number = "hello";

if (val is string) {
    print "It's a string!";
}
```

> [!IMPORTANT]
> Aura does not have an `any` or `unknown` type in the language design to ensure complete type safety across the entire codebase.
