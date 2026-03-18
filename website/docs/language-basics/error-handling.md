---
title: Error Handling
sidebar_position: 8
---

# Error Handling

Aura provides a structured way to handle runtime errors using `try`, `catch`, `finally`, and `throw`.

## The `try...catch` Block

You can wrap code that might throw an error in a `try` block. If an error occurs, the execution jumps to the `catch` block.

```aura
try {
    // Code that might fail
    let user = fetchUser(123);
    print user.name;
} catch (e: Error) {
    // Handle the error
    print "Error occurred: " + e.message;
}
```

## Throwing Errors

Use the `throw` keyword to manually trigger an error. You can throw new `Error` objects or any custom error subclass.

```aura
function divide(a: number, b: number): number {
    if (b == 0) {
        throw new Error("Division by zero");
    }
    return a / b;
}
```

## The `finally` Block

The `finally` block contains code that will always execute, regardless of whether an error was thrown or caught. This is ideal for cleaning up resources like file handles or network connections.

```aura
try {
    openFile();
    processFile();
} catch (e: Error) {
    print "Error processing file";
} finally {
    closeFile(); // This always runs
}
```
