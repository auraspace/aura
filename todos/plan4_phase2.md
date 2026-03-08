# Plan 4 Phase 2: Async Executor / Scheduler

## 📌 Goals

Build the `scheduler/` to handle Aura `Promises` (async tasks). Implement a
work-stealing multi-threaded runtime that can schedule tasks across worker
threads.

## 📝 Tasks

- [x] Define `Task` struct wrapping a pinned future (`scheduler/task.rs`)
- [x] Define `Promise<T>` — the user-facing future primitive (`scheduler/promise.rs`)
- [x] Implement single-threaded `Executor` with run-to-completion loop (`scheduler/executor.rs`)
- [x] Implement `WorkStealingScheduler` with per-thread queues and steal logic (`scheduler/scheduler.rs`)
- [x] Expose public API via `scheduler/mod.rs`
- [x] Wire scheduler into `src/runtime/mod.rs`
- [x] Write unit tests for task submission, completion, and work-stealing
- [x] Verify all tests pass with `cargo test`
