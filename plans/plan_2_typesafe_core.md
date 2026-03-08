# Plan 2: The "Type-Safe Core" (Sema-First)

**Focus**: _Solidifying the strict, TypeScript-inspired Type System._

Since Aura has no `any` type and supports structural typing, the Type Checker is the most complex part. This plan ensures the "brain" of the compiler is perfect before worrying about machine code.

## Phases

1.  **Phase 1: Advanced AST & Sema**: Build out the full parser and semantic analyzer, including generics, union types, and structural identity check.
2.  **Phase 2: Headless Validation**: Use unit tests to verify that valid Aura code passes Sema and invalid code (like using `any`) fails with descriptive errors.
3.  **Phase 3: Symbolic Execution / Interpreter**: Build a simple Tree-Walk Interpreter to run Aura code directly from the AST/Sema state.
4.  **Phase 4: IR Transition**: Lower the validated Sema output into the SSA IR.
5.  **Phase 5: Backend Integration**: Connect the IR to the ARM64 backend once the type system is feature-complete.

> [!IMPORTANT]
> This is the "Safety first" approach, ideal for ensuring the language design remains consistent and bug-free.
