---
title: String
sidebar_position: 2
---

# String Module

The `std/string` module provides utility functions for manipulating and analyzing strings.

## String Class

Most string utility functions can be called either statically via the `String` class or as a method on the string instance (dot notation).


### Static Methods

#### `len(s: string): number`

Returns the length of the string `s`.

#### `charAt(s: string, i: number): string`

Returns the character at the given index `i`.

#### `substring(s: string, start: number, end: number): string`

Returns a substring from `start` to `end` (exclusive).

#### `indexOf(s: string, target: string): number`

Returns the index of the first occurrence of `target` in `s`, or `-1` if not found.

#### `toUpper(s: string): string`

Converts the string to uppercase.

#### `toLower(s: string): string`

Converts the string to lowercase.

#### `trim(s: string): string`

Trims whitespace from both ends of the string.

### Example

```aura
import { String } from "std/string.aura";

let s = "  Hello Aura  ";

// Static call
let trimmed = String.trim(s); 

// Or method call
let trimmed2 = s.trim();

print(s.trim().toUpper()); // "HELLO AURA"
```
