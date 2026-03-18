---
title: Comments
sidebar_position: 0
---

# Comments

Aura supports several styles of comments for documenting your code and temporarily disabling code execution.

## Single-line Comments

Use `//` to start a single-line comment. Everything from the `//` to the end of the line will be ignored by the compiler.

```aura
// This is a single-line comment
let x = 10; // You can also put comments at the end of a line
```

## Multi-line Comments

Use `/*` to start a multi-line comment and `*/` to end it. These are useful for longer explanations or for commenting out large blocks of code.

```aura
/*
   This is a multi-line comment.
   It can span multiple lines.
*/
let y = 20;
```

## Documentation Comments

Documentation comments are special comments used to generate documentation for your code. They attach to the declaration that immediately follows them.

### Line Documentation

Use `///` for single-line documentation comments.

```aura
/// Adds two numbers together
function add(a: i32, b: i32): i32 {
    return a + b;
}
```

### Block Documentation

Use `/**` to start a block documentation comment and `*/` to end it.

```aura
/**
 * A simple Animal class.
 * This represents a generic animal with a name.
 */
class Animal {
    private name: string;
    // ...
}
```
