---
title: Data Types
sidebar_position: 3
---

# Enums

Enums (enumerations) allow you to define a set of named constants. Using enums can make it easier to document intent, or create a set of distinct cases.

## Defining an Enum

You can define an enum using the `enum` keyword.

```aura
enum Color {
    Red,
    Green,
    Blue,
}
```

By default, the values of an enum start from `0` and increment by `1` for each member.

## Explicit Values

You can also explicitly set the value of an enum member.

```aura
enum Status {
    Pending = 0,
    InProgress = 5,
    Completed = 10,
    Failed = -1,
}
```

If a member doesn't have an explicit value, it will increment from the previous member's value.

## Using Enums

You can access enum members using the dot notation.

```aura
let red: Color = Color.Red;
let status: Status = Status.InProgress;

if (status == Status.Completed) {
    print("Task completed!");
}
```

## Enum as Types

Enums can be used as types for variables, function parameters, and return values.

```aura
function getColorName(color: Color): string {
    if (color == Color.Red) return "Red";
    if (color == Color.Green) return "Green";
    if (color == Color.Blue) return "Blue";
    return "Unknown";
}
```
