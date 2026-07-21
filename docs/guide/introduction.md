---
title: Introduction
section: Start
order: 10
summary: What Aura is, who it is for, and how the docs fit with RFCs.
---

# Introduction

**Aura** is a statically typed, compiled language for services, CLIs, workers, and libraries that ship as a **single native executable**.

You write with a **class-based** object model, **null-safe types**, and **lightweight tasks** with a tracing GC. The **toolchain is Rust**; native code is produced today via a **C backend** (LLVM is the longer-term path).

## Product one-liner

> Class-based productivity, concurrent tasks with GC, one binary to deploy.

## Who these docs are for

| You are…                         | Start with…                                 |
| -------------------------------- | ------------------------------------------- |
| Trying Aura for the first time   | [Getting started](./getting-started.md)     |
| Learning the surface language    | [Language tour](./language-tour.md)         |
| Quick syntax lookup              | [Syntax cheatsheet](./syntax-cheatsheet.md) |
| Using the CLI day to day         | [CLI](./cli.md)                             |
| Std packages (`io`, `assert`, …) | [Standard library](./standard-library.md)   |
| Packaging multi-file projects    | [Packages](./packages.md)                   |
| Common questions                 | [FAQ](./faq.md)                             |
| Reading design decisions         | [RFCs](/rfc)                                |

## Docs vs RFCs

|        | **User docs** (this section) | **RFCs**                |
| ------ | ---------------------------- | ----------------------- |
| Goal   | Teach and reference          | Lock design decisions   |
| Tone   | How-to and tour              | Rationale and contracts |
| Source | `docs/guide/`                | `docs/rfc/`             |

When a guide summarizes behavior, the RFC remains the source of truth for edge cases and future changes. Links like [RFC-000](/rfc/000) take you there.

## Status today

- Vision is locked in **[RFC-000](/rfc/000)** (Accepted).
- MVP language surface is tracked in **[RFC-001 §6.0](/rfc/001)**.
- First public alpha: **`0.1.0-alpha`** — install with the [one-liner](./install.md); CLI supports `new` / `init` / `check` / `build` / `run` / `test` / `version` (C backend + embedded runtime).

See also the public [roadmap](./roadmap.md) notes and [0.1.0-alpha freeze](https://github.com/auraspace/aura/blob/main/docs/releases/0.1.0-alpha.md).

## Next

Continue with [Getting started](./getting-started.md) to run `Hello, Aura` on your machine.
