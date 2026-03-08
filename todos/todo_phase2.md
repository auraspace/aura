# Todo: Phase 2 - Direct Codegen (ARM64)

**Focus**: Generate ARM64 assembly directly from the AST for a "Hello World" style arithmetic script.

- [x] Define `AArch64` Register set and basic assembly emitter (`src/compiler/backend/arm64/asm.rs`)
- [x] Implement a simple code generator that walks the AST and emits assembly (`src/compiler/backend/arm64/codegen.rs`)
- [x] Support `let` bindings (stack allocation for now)
- [x] Support binary expressions (`+`, `-`)
- [x] Support `print` (via a call to a runtime function or a syscall)
- [x] Update CLI to output `.s` file
