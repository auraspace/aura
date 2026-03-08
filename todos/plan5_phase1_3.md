# Plan 5 Phase 1 & 3: IR Specification & Text Format

## 📌 Goals

Solidify the "Contract" between the frontend compiler (AST -> IR) and backend compiler (IR -> ARM64). This requires making the IR serializable to a human-readable text format, allowing isolated testing of the frontend and backend.

## 📝 Tasks

- [x] Implement `std::fmt::Display` for `IrModule`, `IrFunction`, `BasicBlock`, `Instruction`, `Operand`, and `IrType` in `src/compiler/ir/instr.rs`.
- [x] Add a CLI flag or standalone utility in `src/main.rs` to output the IR text format for a given Aura source file (e.g., `aura --emit-ir <file>`).
- [x] Write integration tests that confirm the IR formatting works and produces expected output for simple programs.
