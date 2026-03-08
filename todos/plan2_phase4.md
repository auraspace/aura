# Plan 2 Phase 4: IR Transition (SSA Lowering)

## 📌 Goals

Lower the validated AST/Sema state into a robust SSA-based Intermediate Representation. This includes complex objects, method dispatch, and flow-sensitive narrowing in the IR.

## 📝 Tasks

- [x] Support `Class` and `Object` representation in IR
- [x] Implement VTable or Name Mangling for Method Dispatch in IR
- [x] Implement IR lowering for `Expr::TypeTest` using type tags
- [x] Support String representation and basic operations in IR
- [x] Implement constant folding and basic optimizations in IR
- [x] Add `ir_tests.sh` to verify IR output against expected results
