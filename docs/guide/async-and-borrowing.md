---
title: Async, tasks & borrowing
section: Language
order: 37
summary: Bounded async functions, task handles, channels, cancellation, and ref values.
---

# Async, tasks & borrowing

The current compiler supports a bounded async/task surface. Async functions
are lowered into runtime task frames; `await` may suspend and resume the
current task without blocking a worker.

## Async functions and await

Mark a function `async` when it may suspend. Calling it produces a task-like
value whose inner result is obtained with `await` inside another async function
or a spawned task:

```aura
async fun answer(): Int {
  return 42
}

async fun readAnswer(): Int {
  val value: Int = await answer()
  return value
}
```

`await` is only valid in an async function or spawned task body. Borrowed
values cannot cross an await boundary. The implementation supports multiple
bounded suspension points and typed task outcomes, while arbitrary async
control-flow/capture combinations remain implementation-limited.

## Spawn, join, and cancellation

`spawn { ... }` creates a `TaskHandle<T>` for the spawned body. `join` observes
the task outcome and `cancel` requests cooperative cancellation:

```aura
async fun work(): Int {
  return 7
}

fun main() {
  val task: TaskHandle<Int> = spawn {
    return await work()
  }
  join(task)
  cancel(task)
}
```

Cancellation is not preemptive. A task observes it at scheduler boundaries;
`std.task.isCancelled()` reports the current request, and the `std.task`
package also provides `joinTask`, `cancelTask`, `cancelAfter`, and
`linkCancellation` wrappers.

## Channels

`Channel<T>(capacity)` is a bounded FIFO channel. `send` and `receive` are
async operations, and `close` is idempotent:

```aura
async fun producer(channel: Channel<String>): Unit {
  channel.send("ready")
  channel.close()
}
```

Channels require a positive capacity. Payloads cross an ownership boundary, so
borrowed `ref` values cannot be sent or retained by a channel. The shipped
MVP covers bounded `Int`, `String`, and class payloads.

## Scoped `ref` values

`ref T` is a non-owning, non-null scoped reference. It is useful for reading or
mutating an existing value without transferring ownership:

```aura
fun lengthOf(items: Array<Int>): Int {
  val view: ref Array<Int> = items
  return view.len
}
```

Borrow rules are lexical:

- `ref T` cannot be nullable, returned, stored in a class field, or captured by
  a lambda that may outlive the borrow.
- A borrow cannot cross `await`, `spawn`, task storage, channel send, or channel
  creation boundaries.
- Assignment and return checks reject references that would outlive their
  source value.
- Array and collection views are explicit borrowed values; use an owning
  `clone()` or snapshot when the value must survive the source scope.

The borrow checker is intentionally conservative in the current C backend;
general async ownership and richer borrow forms remain future work.

## Related APIs

- [Standard library](./standard-library.md) for `std.task`, `std.time`,
  `std.sync`, `std.stream`, and `std.net`.
- [Arrays](./arrays.md) for ownership and iteration behavior.
- [Control flow & errors](./control-flow-and-errors.md) for task outcomes and
  exception handling.
- [RFC-003](/rfc/003) and [RFC-006](/rfc/006) for normative design context.
