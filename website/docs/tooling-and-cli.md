---
title: 8. Tooling & CLI
sidebar_position: 8
---

# Tooling & CLI

Aura comes with a powerful command-line interface.

## The Aura Compiler (`aura`)
Use the `aura` CLI to test and run your applications:
- `aura build main.aura` - Compiles to binary.
- `aura run main.aura` - Runs the code immediately via JIT/interpreter.

## The REPL
Run `aura repl` to open the interactive Read-Eval-Print Loop.

## WebAssembly (WASM) Integration
You can compile your standard Aura modules via WASM using target decorators or CLI flags, allowing for easy frontend execution.
