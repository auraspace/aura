---
title: Math
sidebar_position: 3
---

# Math Module

The `std/math` module provides common mathematical utilities.

## Math Class

### Static Methods

#### `abs(n: number): number`

Returns the absolute value of the integer `n`.

#### `max(a: number, b: number): number`

Returns the larger of two integers.

### Example

```aura
import { Math } from 'std/math.aura';

print(Math.abs(-42)); // 42
print(Math.max(10, 20)); // 20
```
