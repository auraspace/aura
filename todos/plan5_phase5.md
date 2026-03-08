# Plan 5 Phase 5: Platform Expansion (x86_64)

## 📌 Goals

Demonstrate the benefits of the IR Contract by implementing an entirely new backend for the `x86_64` architecture. Because the frontend simply lowers to SSA IR, the only work required is to build an `IrCodegen` consumer that translates IR instructions into x86_64 native assembly.

## 📝 Tasks

- [x] Implement `x86_64/asm.rs` for generating x86_64 assembly boilerplate (prelude, sysv64 ABI calling conventions, etc).
- [x] Implement `x86_64/ir_codegen.rs`, a consumer of `IrModule` that translates `Instruction` instances into x86_64 assembly instructions (`mov`, `add`, `call`, etc).
- [x] Expose the new `x86_64` backend in `src/compiler/backend/mod.rs` and `src/main.rs`, adding a conditional compilation or command-line flag (`--target x86_64`) to select it.
- [x] Ensure `_aura_alloc` and `_aura_write_barrier` calls are correctly emitted using the System V AMD64 ABI.
- [x] Write integration tests verifying x86_64 codegen works exactly like ARM64.
