# Aura v0.1.1-alpha Workstreams

This directory decomposes the mandatory alpha scope into 72 S/M tasks across
10 workstreams. Every task must leave the repository buildable and add or
update a focused test.

| File                                                                     | Workstream                          |  Tasks |
| ------------------------------------------------------------------------ | ----------------------------------- | -----: |
| [01-contract-and-harness.md](01-contract-and-harness.md)                 | Contract and test harness           |      6 |
| [02-backend-driver.md](02-backend-driver.md)                             | Backend driver and C backend        |      5 |
| [03-runtime-abi-async.md](03-runtime-abi-async.md)                       | Runtime ABI and async state machine |      8 |
| [04-spawn-outcomes.md](04-spawn-outcomes.md)                             | Spawn captures and task outcomes    |      6 |
| [05-async-io.md](05-async-io.md)                                         | Async I/O                           |      6 |
| [06-build-profiles-cache-targets.md](06-build-profiles-cache-targets.md) | Profiles, cache, and targets        |      8 |
| [07-attributes-derives.md](07-attributes-derives.md)                     | Attributes and derives              |      6 |
| [08-race-detector.md](08-race-detector.md)                               | Minimal race detector               |      5 |
| [09-registry-publish-update.md](09-registry-publish-update.md)           | Registry, publish, and self-update  |      8 |
| [10-ffi.md](10-ffi.md)                                                   | Extended FFI                        |      6 |
| [11-http-server.md](11-http-server.md)                                   | Minimal HTTP server                 |      8 |
| **Total**                                                                |                                     | **72** |

```text
01 → 02 → 03 → 04 → 05 → 11
       ├──────────────→ 06
       ├──────────────→ 07
       └──────────────→ 10
04 → 08
06 → 09
```

- One task is one reviewable S/M change.
- Each task has an objective, implementation checklist, acceptance criteria,
  verification steps, and dependency notes.
- Task descriptions intentionally describe responsibilities and contracts, not
  concrete source paths. Later tasks may refactor the implementation layout.
- Each task adds a focused positive or negative test.
- Shared compiler/runtime surfaces have one owner per workstream; cross-
  workstream edits require integration review.
- Checkpoints are integration gates, not additional task IDs.
