# v0.1.1-alpha completion backlog

The original 72 workstream tasks are complete as bounded slices. This file
tracks the remaining work required before the contract matrix can claim the
broader v0.1.1-alpha surface. Each item is independently reviewable and has a
single primary owner; an item is not complete until its implementation, focused
test, matrix evidence, and debt/docs updates land together.

## Dependency graph

```text
A4-CF ─┬─> A5/A6 richer lowering ─> S2/S5 full captures/outcomes ─> H5
       └─> IO2/IO3 full async operations ──────────────────────────> H5/H7
F3/F4 compiler surface ────────────────────────────────────────────> H7
U2/U5/U7 production registry policy ─> REL release matrix/signing
```

## Tasks

| ID  | Owner                                   | Scope                                                                                   | Exit evidence                                                                                    | Status |
| --- | --------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | ------ |
| G1  | Compiler Expert                         | General async state-machine lowering for branches, loops, and arbitrary await placement | Native corpus for control-flow awaits, deterministic state dump, failure/cancel/GC tests         | Open   |
| G2  | Compiler Expert + Runtime               | Full task capture lowering for arrays, classes, functions, mutation, and cancellation   | End-to-end Aura fixtures under ASAN/UBSAN with forced GC and repeated spawn/join                 | Open   |
| G3  | Runtime & Integration                   | Scheduler-integrated file and TCP operations with completion/cancellation handles       | Async file/TCP Aura fixtures, peer failure, backpressure, and no double-close sanitizer evidence | Open   |
| G4  | Runtime & Integration + Compiler Expert | Async HTTP handlers, keep-alive across suspension, and response backpressure            | Aura-level HTTP server example and concurrent/cancelled handler tests                            | Open   |
| G5  | Compiler Expert + Runtime               | Compiler-exposed network/HTTP FFI bindings with safe ownership rules                    | `std.net`/HTTP package API, compile/type diagnostics, installed CLI health example               | Open   |
| G6  | Tooling + Release                       | Production registry compatibility, trust/signing policy, and cross-host acceptance      | Network publish/update smoke, signature verification, Linux arm64/Windows policy evidence        | Open   |
| G7  | Runtime & Integration                   | Complete async/FFI boundary audit and foreign-value support decision                    | Matrix rows updated from tests; no unsupported claim hidden in RFC/docs/debt                     | Open   |
| G8  | Test & Corpus Manager                   | Cross-workstream acceptance corpus and clean-host reproducibility                       | One-command report with no deferred alpha-required stages                                        | Open   |

## Completion rule

The release cannot be called complete while any matrix row is `partial`,
`deferred`, or `blocked`, or while the alpha harness reports a deferred stage.
Bounded slices must remain documented as partial rather than being promoted by
metadata-only edits.
