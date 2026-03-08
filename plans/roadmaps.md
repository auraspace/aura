# Aura Implementation Roadmaps Index

This directory contains five strategic plans to build the **Aura** programming language, each catering to different priorities.

## 🗺️ Available Plans

1.  [**Plan 1: Vertical Slice**](plan_1_vertical_slice.md) - Focus: _End-to-End MVP (Source to Binary)._
2.  [**Plan 2: Type-Safe Core**](plan_2_typesafe_core.md) - Focus: _Strict Type System & Semantic Analysis._
3.  [**Plan 3: DX & Tooling**](plan_3_dx_tooling.md) - Focus: _LSP and Developer Experience._
4.  [**Plan 4: Systems Runtime**](plan_4_systems_runtime.md) - Focus: _High-performance GC & Scheduler._
5.  [**Plan 5: Parallel Contract**](plan_5_parallel_contract.md) - Focus: _Decoupled IR-based Development._

---

## 🎯 Comparison Summary

| Plan                | Priority    | Risk                  | Best For...               |
| :------------------ | :---------- | :-------------------- | :------------------------ |
| **Vertical Slice**  | Speed       | Low Architecture      | Prototyping               |
| **Type-Safe Core**  | Correctness | High Backend Delay    | Complex Language Features |
| **DX & Tooling**    | Usability   | Performance Delay     | Growing a Community       |
| **Systems Runtime** | Performance | High Complexity       | High-Throughput Apps      |
| **Parallel**        | Scale       | Integration Conflicts | Multi-person Teams        |
