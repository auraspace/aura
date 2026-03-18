---
title: Core
sidebar_position: 1
---

# Core Module

The `std/core` module provides fundamental classes and exceptions used across the Aura language.

## Error Class

The `Error` class is the base class for all errors in Aura.

### Constructor

```aura
public constructor(message: string)
```

Creates a new `Error` instance with the specified message.

### Methods

#### `toString(): string`

Returns the error message as a string.

### Example

```aura
import { Error } from "std/core.aura";

let err = new Error("Something went wrong");
print(err.toString());
```
