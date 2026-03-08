# Language Design Rules

All development related to the Aura language's compiler, syntax, and features must strictly adhere to the specifications and philosophy defined in the project's documentation.

## 📋 General Principles

1.  **Syntax Compliance**: Every implemented feature must follow the syntax outlined in `docs/syntax.md`. If a feature is not yet documented, it must be proposed and documented there first.
2.  **Design Philosophy**: Implementation must align with the vision in `docs/overview.md`:
    - **Safety**: Ensure non-nullable types by default and strict type checking.
    - **Performance**: Optimize for efficient native code generation.
    - **Simplicity**: Aim for TypeScript-like developer experience within a static compilation model.
3.  **No Type Escapes**: The keywords `any` or `unknown` are strictly forbidden in Aura. Use generics, union types, or interfaces for flexibility.
4.  **Keyword Consistency**: Use the `function` keyword for function declarations, as specified in the syntax guide. Do not use `fn`.
5.  **Strict OOP**: Follow the OOP model precisely:
    - Explicit `override` keyword for method overrides.
    - Support for `interface` (structural typing) and `class` (nominal/structural mix).
    - Single inheritance with `extends`.
6.  **ARM64 Optimization**: When working on the backend, prioritize ARM64 (AArch64) as the primary target. Ensure code generation is idiomatic for ARM64 before considering x86_64.

## 🛠 Feature Checklist

Before completing any task related to language features, verify:

- [ ] Is the syntax identical to `docs/syntax.md`?
- [ ] Does it maintain "No `any`" rule?
- [ ] Does it support the expected OOP semantics (access modifiers, etc.)?
- [ ] Is it compatible with the Generational GC and memory model?
- [ ] If it's an async feature, does it use the `Promise<T>` model?

## ⚠️ Documentation Sync

If implementing a change that evolves the language beyond its current specs, YOU MUST update `docs/syntax.md` or `docs/overview.md` as part of the same task.
