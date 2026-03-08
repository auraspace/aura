# Todo: Phase 5 - Refinement & SSA IR

**Focus**: Introduce the Middle-end (SSA IR) and Semantic Analysis.

- [x] Define the IR (Intermediate Representation) structures in `src/compiler/ir/instr.rs`
- [x] Implement an IR Builder in `src/compiler::ir::builder.rs`
- [x] Implement a lowering pass from AST to IR
- [x] Implement Semantic Analysis (Type checking, Scope resolution) in `src/compiler/sema/`
- [x] Update Backend to use IR (ARM64 `IrCodegen` implemented)
- [x] Implement Class/Object support in IR and Lowerer (MemberAccess, MethodCall, etc.)
- [x] Implement basic Optimizations on the IR (e.g., Constant Folding + Propagation)
- [x] Verify with recursive Fibonacci test case (SSA IR verified!)
- [x] Verify with Points class test case using IR pipeline
