# Plan 4: The "Systems Runtime" (Runtime-First)

**Focus**: _High-performance GC and Async Scheduler._

Aura wants to be "The power of C++". This plan focuses on the "heart" of the language: the memory and execution model.

## Phases

1.  **Phase 1: Generational GC**: Implement the `heap.rs` and `sweep.rs` in Rust. Test it with manual allocations.
2.  **Phase 2: Async Executor**: Build the `scheduler/` to handle Aura `Promises`. Implement a work-stealing multithreaded runtime.
3.  **Phase 3: FFI & System Hooks**: Implement the macOS/Linux syscall wrappers for I/O and networking.
4.  **Phase 4: Compiler-Runtime Contract**: Define how the compiler should generate code that interacts with the GC (Stack maps, write barriers).
5.  **Phase 5: Full Codegen**: Build the ARM64 emitter to target the mature runtime's ABI.
