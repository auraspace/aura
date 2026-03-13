# Plan 4 Phase 5: Full Codegen

## 📌 Goals

Build the ARM64 emitter to target the mature runtime's ABI. This involves fully implementing the IR -> ARM64 native assembly generation, correctly wiring up GC allocation, write barriers, function calls, and StackMaps so that output assemblies interact safely with the Rust GC and scheduler logic.

## 📝 Tasks

- [x] Update `src/compiler/backend/aarch64_apple_darwin/ir_codegen.rs` to implement `Instruction::Alloc` via calls to `aura_alloc` (the runtime API) instead of just stack reservation.
- [x] Update `ir_codegen.rs` to implement `Instruction::WriteBarrier` by calling a placeholder native `aura_write_barrier` or inline barrier logic.
- [x] Define runtime standard library stubs in Rust (e.g., `aura_alloc`, `aura_write_barrier`, `print_num`, `print_str`).
- [x] Write a test pipeline ensuring a simple Aura program correctly lowers -> codegens -> links with runtime stubs -> executes safely.
