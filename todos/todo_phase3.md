# Todo: Phase 3 - The Static Linker & Runtime

**Focus**: Bundle a tiny runtime with the generated assembly into an executable.

- [x] Create a minimal C-based runtime with `print_num` (`src/runtime/runtime.c`)
- [x] Implement a `Driver` in Rust to handle assembly, object file creation, and linking (`src/compiler/backend/arm64/driver.rs`)
- [x] Automate the `as` (assembler) and `ld` (linker) calls using `std::process::Command`
- [x] Verify execution of the compiled Aura binary on ARM64 Mac
