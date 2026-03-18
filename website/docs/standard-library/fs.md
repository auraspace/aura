---
title: File System (FS)
sidebar_position: 4
---

# FS Module

The `std/fs` module provides synchronous and asynchronous file system utilities.

## FS Class

### Static Methods

#### `open(path: string, flags: number, mode: number): number`

Opens a file and returns its file descriptor.

#### `close(fd: number): void`

Closes the specified file descriptor.

#### `read(fd: number, size: number): string`

Reads up to `size` bytes from the file descriptor.

#### `write(fd: number, content: string): number`

Writes the specified content to the file descriptor.

#### `readFileSync(path: string): string`

Synchronously reads the entire contents of a file as a string.

#### `writeFileSync(path: string, content: string): void`

Synchronously writes a string to a file, creating it if it doesn't exist.

### Example

```aura
import { FS } from "std/fs.aura";

// Write content to a file
FS.writeFileSync("hello.txt", "Hello from Aura!");

// Read it back
let content = FS.readFileSync("hello.txt");
print(content);
```
