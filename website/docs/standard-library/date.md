---
title: Date & Time
sidebar_position: 7
---

# Date Module

The `std/date` module provides classes for date and time handling.

## Date Class

### Constructor

- `constructor(val: i64 | string | void)`: Creates a new date instance. If `void`, defaults to the current time.

### Methods

- `getTime(): i64`: Returns the timestamp in milliseconds.
- `getFullYear(): number`: Returns the full year.
- `getMonth(): number`: Returns the month (0-11).
- `getDate(): number`: Returns the day of the month (1-31).
- `getHours(): number`, `getMinutes(): number`, `getSeconds(): number`: Time getters.
- `toISOString(): string`: Returns an ISO 8601 string.
- `toString(): string`: Returns a human-readable string.

### Static Methods

- `static now(): i64`: Returns the current timestamp.
- `static parse(s: string): i64`: Parses a date string.

### Example

```aura
import { Date } from "std/date.aura";

let now = new Date();
print(now.toISOString());
```
