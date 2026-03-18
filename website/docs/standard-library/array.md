---
title: Array
sidebar_position: 8
---

# Array Module

The `std/array` module provides utilities for working with arrays.

## Array Class

Functions in the `Array` module can be called using static syntax or as methods on an array instance.


### Static Methods

#### `len(a: any[]): number`

Returns the length of array `a`.

#### `push(a: any[], item: any): void`

Pushes an item to the end of the array.

#### `pop(a: any[]): any`

Pops an item from the end of the array and returns it.

#### `join(a: any[], sep: string): string`

Joins all elements of an array into a single string with the separator `sep`.

#### `get(a: any[], i: number): any`

Returns the element at the specified index.

### Example

```aura
import { Array } from "std/array.aura";

let arr = [1, 2, 3];

// Static call
Array.push(arr, 4);

// Or method call
arr.push(5);

print arr.join("-"); // "1-2-3-4-5"
```
