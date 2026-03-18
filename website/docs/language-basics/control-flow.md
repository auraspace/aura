---
title: Control Flow
sidebar_position: 6
---

# Control Flow

Aura provides standard control flow structures to manage the execution of your code.

## Conditionals (`if/else`)

Use `if` and `else` to perform actions based on a boolean condition.

```aura
let age = 18;

if (age >= 18) {
    print "Access granted";
} else {
    print "Access denied";
}
```

## Loops (`while`)

The `while` loop continues to execute as long as its condition remains `true`.

```aura
let count = 0;
while (count < 5) {
    print count;
    count = count + 1;
}
```


## Next Steps

Aura's control flow is designed to be simple and predictable. For managing runtime failures and exceptions, see the [Error Handling](error-handling.md) guide.

> [!NOTE]
> Currently, Aura primarily supports `while` loops. Iteration over arrays and other collection types using `for` loops is planned for future releases.
