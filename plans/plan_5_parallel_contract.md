# Plan 5: The "Go-Style" Parallel (Contract-First)

**Focus**: _Decoupled development between Frontend and Backend._

Modeled after the Go compiler, this plan uses a strictly defined Intermediate Representation (IR) as the boundary.

## Phases

1.  **Phase 1: IR Specification**: Define the SSA IR instructions (`instr.rs`) and the builder API. This is the "Contract".
2.  **Phase 2: Parallel Streams**:
    - **Frontend Team**: Works on `lexer`, `parser`, `sema` -> `IR Builder`.
    - **Backend Team**: Works on `IR` -> `ARM64 ASM` and `Runtime`.
3.  **Phase 3: IR Verification Tool**: Build a tool that can read/write the IR in a text format for testing backend and frontend separately.
4.  **Phase 5: Platform Expansion**: Once ARM64 is solid via the IR interface, adding `x86_64` becomes a matter of adding a new consumer for the same IR.
