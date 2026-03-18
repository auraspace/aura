---
id: introduction
title: 1. Introduction
sidebar_position: 1
---

# Introduction

Welcome to the official documentation for the **Aura** programming language. Whether you are a seasoned systems programmer or coming from high-level languages like TypeScript, Go, or Python, this guide will help you understand the core concepts, syntax, and tooling that make Aura unique.

## What is Aura?

Aura is a statically typed, monolithic, and inherently fast systems programming language. It is built from the ground up to offer the ultimate developer experience without sacrificing runtime performance.

Unlike many other modern languages that rely on fragmented ecosystems of third-party tools, Aura takes a "batteries-included" approach. Everything you need to parse, analyze, compile, and run your code is embedded directly into a single, cohesive toolchain.

## Core Philosophy

Aura was designed around three primary pillars:

1. **The Simplicity of Go**: Writing Aura code feels straightforward. The language avoids overly complex, magical abstractions, favoring a clean and minimal syntax. This allows developers to onboard quickly and focus on solving problems rather than learning convoluted language features.
2. **The Tooling of TypeScript**: Developer experience is a first-class citizen. Aura provides an intelligent, built-in language server (LSP) that natively supports hover, completion, and go-to-definition, giving you IDE-level productivity out of the box.

## Key Features

- **Standalone Compiler & Runtime**: The Aura compiler converts your code directly into a single binary with a statically linked runtime. There are no external virtual machines or heavy runtime frameworks required to deploy your app.
- **Built-in Language Server (LSP)**: Editor support is built directly into the core language architecture. The same compiler that builds your game or backend server also powers your editor via the `aura lsp` command.
- **AArch64 First**: Aura's custom backend infrastructure prioritizes modern ARM architectures (specifically Apple Silicon / `aarch64_apple_darwin`) ensuring lightning-fast execution on modern hardware, alongside solid support for `x86_64`.
- **Integrated Generational GC**: Aura handles memory management natively through its built-in generational Garbage Collector. You get the performance of systems languages without the constant manual memory overhead.
- **WebAssembly Ready**: Aura's compiler and ecosystem are highly portable. By supporting WebAssembly (`wasm32-unknown-unknown`), projects (and even the Aura compiler itself) can seamlessly run inside a browser environment.

## Who is Aura For?

Aura is crafted for engineers who want the reliability and performance of compiled, statically typed languages, but refuse to compromise on modern developer ergonomics. It is ideal for building high-performance CLI tools, custom web services, async task runners, and WebAssembly-powered applications.

## Next Steps

Now that you know what Aura is, it's time to write some code!
Proceed to the **Getting Started** guide to set up the Aura compiler and build your first program.
