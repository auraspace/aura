# Aura Programming Language

Aura is a modern, high-performance, strictly-typed programming language inspired by **TypeScript** and **Rust**. It aims to provide the developer-friendly syntax and OOP features of TypeScript with the safety and performance of a systems language.

This repository (`aura`) contains the core toolchain for Aura, including the compiler, runtime, and standard library, all written in **Rust**.

---

## 🚀 Key Features

- **TypeScript-Inspired Syntax**: Familiar and intuitive for web developers while offering the power of a compiled language.
- **Strictly Typed**: No `any` or `unknown` types. Every value has a known type, ensuring safety and enabling aggressive compiler optimizations.
- **Modern OOP**: Robust support for classes, inheritance, interfaces, and structural typing.
- **Advanced Type System**: Features like union/intersection types, strict nullability, and powerful generics.
- **Zero-Cost Abstractions**: Leveraging Rust and a custom backend infrastructure to provide high performance without sacrificing expressiveness.
- **Custom Backend Architecture**: Similar to Go, Aura uses its own specialized backend and code generator, optimized for fast compilation and efficient execution.
- **Self-Contained Binaries**: Compiles into a single, standalone executable. The runtime is statically linked, allowing you to run the application anywhere without external dependencies.
- **Multi-Architecture Support**: Designed to target **aarch64-apple-darwin**, **x86_64-unknown-linux-gnu**, and **x86_64-pc-windows-msvc** natively.
- **First-Class Tooling**: Designed from the ground up with a focus on Language Server Protocol (LSP) support and modern build tools.

---

## 🎨 Design Philosophy

### 1. Safety without Compromise

Aura eliminates common pitfalls like null-pointer exceptions and implicit type conversions. By removing the `any` type, Aura forces developers to think about their data structures, resulting in more maintainable and bug-free code.

### 2. Performance First

The choice of Rust for the compiler implementation ensures that the toolchain itself is fast and reliable. The language is designed to be compiled to efficient native code, making it suitable for both server-side workloads and performance-critical applications.

### 3. Developer Experience

Aura adopts the best features of modern languages:

- **Async/Await** for easy asynchronous programming.
- **Decorators** for clean metadata and cross-cutting concerns.
- **Pattern Matching** for expressive control flow.
- **Flexible Mixins** via structural typing.

### 4. Deployment Simplicity

Inspired by the Go deployment model, Aura targets simplicity in distribution. By bundling the runtime and core libraries into the binary at compile time, we ensure that an Aura application is just one file. This "xcopy deployment" model means you can deploy your application by simply copying a single executable to the target server or machine.

### 5. Hardware Support & Roadmap

Aura's custom backend is designed for cross-platform portability. Our roadmap includes:

- **aarch64-apple-darwin (Priority)**: The immediate focus is providing a world-class experience on aarch64-apple-darwin architecture (Apple Silicon), leveraging its efficiency and modern instruction set.
- **x86_64**: Full support for standard 64-bit Intel/AMD systems (**Linux** and **Windows**) is planned as the next milestone.

---

## 🛠 Project Structure

The Aura toolchain is designed with a modular architecture in Rust. Each component is isolated to allow independent development and testing:

```text
aura/
├── Cargo.toml             # Project manifest and dependencies
├── src/
│   ├── main.rs            # CLI entry point (driver for compiler, lsp, etc.)
│   ├── lib.rs             # Core library exporting compiler and runtime
│   ├── compiler/          # High-level compiler orchestration
│   │   ├── ast/           # Abstract Syntax Tree definitions
│   │   ├── frontend/      # Lexical analysis and parsing
│   │   │   ├── lexer.rs   # Scanner: Source text -> Tokens
│   │   │   ├── parser.rs  # Parser: Tokens -> AST
│   │   │   └── token.rs   # Token definitions and kinds
│   │   ├── sema/          # Semantic analysis and Validation
│   │   │   ├── checker.rs # Type checking and inference
│   │   │   ├── scope.rs   # Symbol tables and scoping
│   │   │   └── ty.rs      # Aura type system representation
│   │   ├── ir/            # Intermediate Representation (SSA style)
│   │   │   ├── builder.rs # IR construction utilities
│   │   │   └── instr.rs   # Instruction set architecture-agnostic
│   │   └── backend/       # Native code generators
│   │       ├── codegen.rs # Common backend traits and logic
│   │       ├── aarch64_apple_darwin/ # Primary: AArch64 registers & instructions
│   │       │   ├── reg.rs # Register allocator for ARM64
│   │       │   └── asm.rs # Assembler/Emitter for ARM64
│   │       ├── x86_64_unknown_linux_gnu/ # Secondary: Intel/AMD Linux
│       └── x86_64_pc_windows_msvc/    # Secondary: Intel/AMD Windows
│   ├── runtime/           # Language Runtime (included in binaries)
│   │   ├── gc/            # Generational Garbage Collector
│   │   │   ├── heap.rs    # Allocation and management
│   │   │   └── sweep.rs   # Garbage identification logic
│   │   ├── scheduler/     # Async executor for Promises/Tasks
│   │   └── ffi/           # Platform-specific system calls (Linux, macOS)
│   └── lsp/               # IDE Support (Language Server)
│       ├── server.rs      # LSP message handler
│       └── handler/       # Hover, Completion, Definition logic
├── stdlib/                # Core library written in Aura (.aura files)
├── docs/                  # Architecture and syntax specs
└── tests/                 # Integration tests (Aura source -> binary execution)
```

---

## 🚦 Getting Started

> [!NOTE]
> The Aura project (Rust implementation) is currently in active development.

For detailed syntax information, please refer to the [Syntax Design](syntax.md) document.

---

## 🤝 Contributing

We welcome contributions from the community! Whether you're interested in compiler design, runtime performance, or documentation, there's a place for you in the Aura project.

_Aura: The power of C++, the safety of Rust, and the beauty of TypeScript._
