---
title: Introduction
section: Start
order: 10
summary: What Aura is, who it is for, and how the docs fit with RFCs.
---

# Introduction

**Aura** is a statically typed, compiled language for services, CLIs, workers, and libraries that ship as a **single native executable**.

You write with a familiar **Java-like** class model, **null-safe types**, and **Go-like** lightweight tasks with a tracing GC. The **toolchain is Rust**; native code is produced today via a **C backend** (LLVM is the longer-term path).

## Product one-liner

> Java-like productivity, Go-like concurrency, one binary to deploy.

## Who these docs are for

| You are… | Start with… |
| -------- | ----------- |
| Trying Aura for the first time | [Getting started](./getting-started.md) |
| Learning the surface language | [Language tour](./language-tour.md) |
| Quick syntax lookup | [Syntax cheatsheet](./syntax-cheatsheet.md) |
| Using the CLI day to day | [CLI](./cli.md) |
| Std packages (`io`, `assert`) | [Standard library](./standard-library.md) |
| Packaging multi-file projects | [Packages](./packages.md) |
| Common questions | [FAQ](./faq.md) |
| Reading design decisions | [RFCs](/rfc) |

## Docs vs RFCs

| | **User docs** (this section) | **RFCs** |
| --- | --- | --- |
| Goal | Teach and reference | Lock design decisions |
| Tone | How-to and tour | Rationale and contracts |
| Source | `docs/guide/` | `docs/rfc/` |

When a guide summarizes behavior, the RFC remains the source of truth for edge cases and future changes. Links like [RFC-000](/rfc/000) take you there.

## Status today

- Vision is locked in **[RFC-000](/rfc/000)** (Accepted).
- MVP language surface is tracked in **[RFC-001 §6.0](/rfc/001)**.
- The `aura` CLI can **check**, **build**, **run**, and **test** real packages in this repository (C backend + runtime).

See also the public [roadmap](./roadmap.md) notes.

## Next

Continue with [Getting started](./getting-started.md) to run `Hello, Aura` on your machine.
