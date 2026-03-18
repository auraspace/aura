---
title: 6. Advanced Concepts
sidebar_position: 6
---

# Advanced Concepts

Aura is designed for high-performance systems programming with modern abstractions. This section dives deep into its internal mechanisms, from memory management to the compiler pipeline.

## Memory Management & GC

Aura employs a **Generational Garbage Collector** designed for high throughput and low latency. It automates memory safety while providing performance characteristics suitable for systems-level applications.

### Generational Heap Structure

The heap is divided into two primary generations:

- **Young Generation (Nursery)**: Most objects are initially allocated here using a fast bump-pointer allocator. It consists of a "from-space" and a "to-space" (each 2 MiB by default).
- **Old Generation**: Objects that survive multiple collection cycles in the young generation are promoted to the old generation (8 MiB by default).

### Collection Strategy

- **Minor GC (Scavenge)**: Uses Cheney's copying algorithm to move live objects from the young from-space to the to-space or promote them to the old generation. It is extremely fast and has minimal pause times.
- **Major GC (Mark-Compact)**: Triggered when the old generation is full. It performs a mark-and-compact pass to reclaim space and reduce fragmentation.
- **Write Barriers**: Aura's compiler automatically inserts write barriers for pointer assignments to ensure the GC maintains a correct root set across generations.

## Concurrency & Async

Aura's concurrency model is built around **Lightweight Tasks** and a **Work-Stealing Scheduler**, drawing inspiration from modern runtimes like Go and Tokio.

### The Work-Stealing Scheduler

- **Worker Threads**: The runtime spawns a pool of worker threads (matching CPU cores).
- **Local Deques**: Each worker has its own double-ended task queue. Workers push and pop tasks from the back (LIFO) to maximize cache locality.
- **Stealing**: When a worker runs out of tasks, it attempts to "steal" tasks from the front (FIFO) of other workers' queues, ensuring fair load distribution across all cores.

### Promises and Tasks

`async` functions in Aura return `Promises`. These are non-blocking and are polled by the scheduler until completion. Tasks are the unit of execution that the scheduler manages, abstracting away raw OS threads.

## Error Handling

Aura treats errors as first-class citizens, avoiding the overhead and complexity of traditional exceptions.

- **Explicit Returns**: Functions that can fail return multiple values (usually the result and an error object), similar to Go's pattern.
- **Panic Mechanism**: For unrecoverable errors, Aura provides a `panic` mechanism that halts the current task and provides a detailed stack trace for debugging.
- **Zero-Cost Abstractions**: The error handling paths are optimized to ensure that "sunny day" execution paths incur zero performance overhead.

## Foreign Function Interface (FFI)

Aura provides a robust FFI for interoperability with C and system-level libraries.

- **C Linkage**: Aura can call functions exported from C libraries by declaring them with the `extern` keyword.
- **Memory Safety**: While FFI calls are inherently "unsafe," Aura provides wrappers to ensure that GC-managed objects can be passed to C safely without being moved or reclaimed during the call.
- **Static Linking**: The Aura compiler statically links the runtime and any declared FFI dependencies into a single, self-contained binary.

## Compiler Architecture (IR & Codegen)

The Aura compiler follows a modern multi-pass design, culminating in a custom backend for AArch64.

### SSA-based Intermediate Representation (IR)

Aura uses an internal **Static Single Assignment (SSA)** IR. This format enables powerful optimization passes, including:

- Dead code elimination
- Constant folding
- Register pressure reduction

The IR includes high-level instructions for memory allocation (`alloc`), virtual method calls (`call_virtual`), and GC synchronization (`write_barrier`).

### AArch64 Codegen

The primary backend targets `aarch64-apple-darwin` (Apple Silicon). It features:

- **Custom Register Allocator**: Optimizes the use of the 31 general-purpose registers (`x0`-`x30`) and 32 floating-point registers (`d0`-`d31`).
- **Direct Machine Code Emission**: Generates highly optimized assembly directly from the IR, avoiding generic intermediate tools where possible.
- **Static Runtime Integration**: The language runtime (including the GC and scheduler) is written in C and Assembly and is statically embedded into every compiled Aura binary.

## Advanced Type System

Aura's type system combines the safety of static typing with the flexibility of modern type theory features.

### Union Types

Aura supports **Union Types**, allowing a variable to hold values of multiple different types. This is particularly useful for modelling optional data or alternative return paths without complex class hierarchies.

- **Syntax**: `let x: int | string = 10;`
- **Type Narrowing**: The compiler automatically narrows the type of a variable within conditional blocks if it can prove the specific type at runtime (e.g., after an `is` check).

### Generics

Generics allow for writing reusable, type-safe code that works with any data type.

- **Type Parameters**: Functions and classes can be parameterized with types, e.g., `class Box<T> { val: T }`.
- **Compile-time Specialization**: The Aura compiler uses monomorphization or type-erasure depending on the target backend to ensure that generic code remains highly efficient.

## Language Intrinsics

Intrinsics are special functions implemented directly by the compiler. They provide access to low-level CPU instructions or runtime internals that cannot be expressed purely in Aura syntax, such as:

- Atomic operations
- Direct memory access (in specific contexts)
- Performance-critical math functions
