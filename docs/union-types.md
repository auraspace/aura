# Union Types

Union types allow a value to be one of several types. A union type string is created by using the pipe (`|`) symbol to separate each type.

## Definition

For example, `string | i32` is a type that can be either a `string` or an `i32`.

```aura
let x: string | i32 = "Hello";
x = 42; // Valid
```

## Nullable Types

A common use case for union types is to allow a variable to be `null`. Aura uses `Type | null` to represent nullable types.

```aura
let name: string | null = null;
name = "Aura";
```

## Type Narrowing

Type narrowing is the process of refining a union type to a more specific type within a conditional block. In Aura, you use the `is` operator within an `if` statement to narrow a type.

### The `is` Operator

The `expr is Type` expression returns `true` if the value of `expr` is of the specified `Type`.

### Narrowing in `if` Blocks

When you use `is` in the condition of an `if` statement, the Aura compiler automatically narrows the type of the variable within the `then` and `else` branches.

```aura
function process(value: string | i32) {
    if (value is string) {
        // Here, 'value' is narrowed to 'string'
        print "String length: " + value.len();
    } else {
        // Here, 'value' is narrowed to 'i32' (the remaining type)
        print "Number: " + value;
    }
}
```

### Complex Unions

If a union has more than two types, narrowing a specific type in the `if` branch will narrow the `else` branch to a union of the remaining types.

```aura
let x: string | i32 | boolean = ...;

if (x is string) {
    // x is string
} else {
    // x is i32 | boolean
    if (x is i32) {
        // x is i32
    } else {
        // x is boolean
    }
}
```

## Complex Types in Unions

Unions are not limited to basic types; they can also include classes, interfaces, and enums.

### Classes and Interfaces

You can create a union of different classes or interfaces. This is useful for handling multiple related but distinct object types.

```aura
interface Shape {
    function getArea(): f64;
}

class Circle {
    function getArea(): f64 { return 3.14; }
}

class Square {
    function getArea(): f64 { return 1.0; }
}

let shape: Circle | Square | string = ...;

if (shape is string) {
    print "It's a string: " + shape;
} else if (shape is Shape) {
    // Narrowed to the 'Shape' interface (structural match)
    print "Area: " + shape.getArea();
}
```

### Enums

Unions can also include enum types.

```aura
enum Status {
    Active,
    Inactive
}

let result: Status | string = Status.Active;
```

## Member Access Rules

> [!IMPORTANT]
> You cannot access fields or methods directly on a union type, even if they are common to all types in the union. You **must** narrow the type using an `if` statement and the `is` operator before accessing any members.

```aura
let x: Circle | Square = ...;

// x.getArea(); // Error: Method not found on union type

if (x is Circle) {
    x.getArea(); // Valid
}
```

## Assignment Rules

A value of type `A` is assignable to a union type `B | C` if `A` is assignable to `B` OR `A` is assignable to `C`.

A union type `B | C` is assignable to a type `A` if BOTH `B` and `C` are assignable to `A`.

```aura
let u: string | i32 = "test";
let s: string = "hello";

u = s; // Valid: string is assignable to string | i32
// s = u; // Error: string | i32 is not assignable to string
```
