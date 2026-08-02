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

async fun adjustedAnswer(): Int {
  val value: Int = (await answer()) + 1
  return value
}
```

`await` is only valid in an async function or spawned task body. An `await`
may be nested in a supported binary/unary expression or call argument,
including aggregate results such as `String` when the surrounding value has a
clone/drop/mark contract; the compiler stores the awaited value in the frame
and evaluates the continuation after resume. Borrowed values cannot cross an await boundary. The implementation supports multiple
suspension points, typed `Result<T, TaskError>` outcomes, generic inferred
spawn captures (including optional primitive locals), aggregate clone/drop, and
typed frame GC hooks. Nested aggregate arrays delegate to their generated inner
hooks, so `Array<Array<Enum>>` does not fall back to shallow `memcpy` ownership.
Multiple awaits in one initializer or assignment are lowered as an ordered chain
of typed frame slots; later operands run only after earlier awaits complete, and
each edge preserves the same failure/cancellation cleanup contract.
Generic free `async fun` declarations are closed before lowering, including
supported suspension CFGs and straight-line no-suspension bodies, and work
through inferred `Task<T>` call sites.
The same substitution reaches frame locals, catch/channel annotations, lambda
types, and aggregate return slots.
Closed generic functions also contribute their spawned-task frames to static
poller discovery after type substitution. Frame, poller, destroy, and GC-mark
symbols are keyed by the concrete capture layout, so separate instantiations
cannot share incompatible storage. Operations that still require an open
value-dependent descriptor are rejected until that descriptor is available.
Expression-form branches such as `if (await ready()) { ... } else { ... }`
also store the condition in a typed frame slot and resume through the value
continuation without evaluating `ready()` twice.
Statement expressions may also suspend inside supported nested positions, such
as `consume(await nextChunk())`; the temporary awaited value is stored in the
frame and released through the same typed ownership hooks even when the outer
call result is discarded.
Nested protected async blocks compose their `finally` continuations in lexical
order; inner cleanup completes before the outer cleanup is resumed.
The general handler CFG lowering covers branches, loops, repeated awaits,
try/catch/finally, cancellation, async class methods, and scheduler-owned
locals. Spawn bodies use the same typed CFG for branch/loop/repeated-await
shapes, including inferred local captures and aggregate frame layouts; mutable
primitive, Array, Fun, and class captures are passed as retained shared-box
references, so mutation remains visible after suspension. A shape
whose capture or aggregate ABI has no generated ownership hooks is rejected
during code generation rather than silently falling back to an unsafe closure.
Nested branch/loop/match locals that shadow an outer binding are alpha-renamed
in the async frame layout, preserving distinct types and values across resume;
lambda bodies continue to use their separate closure environment.
Foreign handles nested in an owned Array,
struct, or generic enum payload use generated retain/drop hooks across a task
boundary. Scheduler objects use explicit executor/channel reference wrappers
when captured or sent through a channel.
For a genuinely open generic or plugin boundary, the C runtime exposes the
versioned `AuraTypeErasedOps`/`AuraTypeErasedValue` contract with explicit
clone, drop, and mark callbacks; unresolved values are never represented as
an implicit integer layout. Open generic async identity/forwarding functions
and awaited `T` locals propagate that descriptor through child task frames and
clone the child descriptor result before releasing the child frame; operations
that inspect or construct values still require a closed monomorph or an
explicit descriptor operation.
The descriptor-backed pollers also check cancellation on every resume, before
starting or polling the next erased child. A cancelled generic chain cannot
advance to a later `await`; frame destruction releases any owned child handle
and drops the erased payload exactly once.

Failure payloads have two layers in the runtime: the normalized `TaskError`
message used by `join`, and an independently cloned raw payload used while a
nested failure is propagated between frames. Both terminal payload storage and
the raw error payload are GC roots until the owning frame is destroyed.
Typed exceptions may be scalar values, Arrays, classes, enums, value structs,
interfaces, function values, or tagged `ForeignHandle<T>` values when their
generated clone/drop/retain hooks are available. Catching one after `await` copies the raw payload into the receiver
frame and preserves its type tag; nested propagation never borrows the child
frame's storage.
Aggregate returns transfer that ownership to the receiving frame or local; the
compiler never leaves a GC root pointing at a returned stack temporary.
`for-in` bindings over owned aggregate arrays clone each element before the
binding crosses a suspension, while foreign handles acquire an explicit retain;
array `get`, `push`, and `set` likewise use generated aggregate clone/drop
hooks, so a read cannot escape as a shallow alias.
This also applies to value structs containing heap-class fields: cloning a
struct does not root its temporary stack copy; the owning frame/result invokes
the struct mark hook while the aggregate is live.
The same contract applies through nested enum payloads: enum mark hooks recurse
into value structs and their aggregate fields before a suspended task is
collected. Async frame marks now dispatch through the generated `Array<T>` mark
hook too, so arrays of enums/structs and nested arrays retain their reachable
heap fields across suspension and forced collection. Once a task completes, its
typed frame mark hook also traverses the terminal `Task<T>` result until the
owning join/result destructor releases it. `join` also preserves nullable primitive payloads (`Int?` and
`Bool?`) by value; nullable heap-class payloads keep their semantic `T?` type
while using the underlying pointer representation for aggregate clone/drop/mark
hooks, including repeated owning joins. Nullable Array payloads use the same
underlying value representation. A forced unwrap used as an Array method
receiver is materialized into an owned temporary before the method call, so
both read and mutating methods receive a stable addressable receiver.
Class-valued exception fields follow the matching error-payload contract: the
propagated copy roots each nested class until its generated exception destructor
releases the root.
Array-valued class exception fields are cloned at throw and catch boundaries so
their owned elements remain valid after the child task is released. Throw
lowering unregisters local array roots before the exception's `longjmp`, while
the copied catch payload registers its own array root until frame cleanup.

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

An awaited `Unit` task may be used as a statement (`await flush()`), including
inside a spawned task. The compiler keeps a task slot in the static frame even
when no result slot is needed; pending, failed, and cancelled outcomes all use
the same resume/cleanup path. A handle-backed HTTP task keeps its foreign
resource pin until the frame cleanup runs, so dropping the lexical handle while
the task is suspended cannot destroy the connection prematurely.

Boolean task results may also drive a control-flow edge directly:

```aura
while (await body.hasMore()) {
  await response.writeChunk("next")
}
```

The condition is stored in the async frame before the branch or loop edge is
entered. Cancellation or failure resumes through the same typed cleanup path;
the condition task is not created again when a suspended frame is polled. When
the awaited child inside `try/finally` is cancelled, the frame records that
child cancellation, executes the synchronous `finally` block, and only then
publishes the cancelled outcome; the child frame is released exactly once.
Cancellation requested while executing another CFG state inside the protected
region follows the same `finally` edge, so synchronous branch/action states do
not bypass cleanup.

When cancellation targets the async frame itself, the generated frame installs
the same typed cancellation hook. The runtime invokes that hook before making
the frame terminal; it re-enters the CFG and runs nested synchronous
`finally` blocks from inner to outer, then returns the cancellation outcome.
This keeps cancellation cleanup consistent whether cancellation comes from an
awaited child or from `cancel(handle)`.

Scheduler-owned locals are frame fields when they remain live across an
`await`. `Task`, `TaskHandle`, and `Channel` fields use explicit executor or
channel ownership references and are released by the typed frame drop hook;
the continuation may safely cancel or otherwise use the same handle after the
await.

An immutable array captured by a spawned task is cloned; a mutable Array uses
an owned shared box rather than a non-owning view. If its
elements are heap classes, the cloned element buffer is registered as a GC
array root until the task is destroyed, so the capture remains valid across
`await` and forced collection.

General CFG spawns apply the same shared-cell rule to mutable `Int`, `Bool`,
`String`, `Array<T>`, and class locals. The outer binding and the spawned frame
retain the cell independently, so mutation remains visible after suspension
and after the outer scope exits; frame teardown releases only its own retain.
Array method calls address the payload stored in the cell, rather than a stale
non-owning view.

`Task<T>` joins expose `Result<T, TaskError>`. For aggregate payloads, including
interface values backed by heap classes, an owning join clones the payload;
repeated joins and forced GC therefore do not alias or invalidate the result.
Nullable reference-like payloads retain their semantic `T?` monomorph in the
generic `Result` layout while sharing `T`'s C representation; the generated
clone/drop/mark hooks operate on that underlying representation.
Interface payloads use generated typed `clone`, `drop`, and `mark` hooks.
Failed `TaskError` values expose the normalized message plus typed exception
name and source
span when available; `taskErrorTypeName`, `taskErrorSpanStart`, and
`taskErrorSpanEnd` expose diagnostic metadata without exposing a borrowed
child-frame pointer. Async CFG frames also carry the throw-span start as the
stable source identity used by `taskErrorSourceId`, so nested propagation does
not erase the origin metadata.
Bounded generic spawn frames install the same typed mark hook for captured
aggregate values before submission, so suspension does not rely only on a
conservative pointer scan. General multi-await result destructors use the same
generated ownership hooks for class, enum, struct, interface, function, and
Array results. Scheduler-owned `Task`, `TaskHandle`, and `Channel` captures
and channel payloads use explicit executor/channel retain-release wrappers.
Function-valued results retain their closure environment until the owning
frame/result is destroyed. Enum, struct, and interface results are cloned
before publication when the source expression is a borrowed constructor or
field value, so terminal drop never frees a stack alias or borrowed String.
If an executor shuts down while such a payload is queued, shutdown detaches
the frame and the payload destructor finishes its cleanup independently; do
not use the invalidated lexical task handle afterward.

Typed catches in async CFG use scoped bindings. Reusing a catch name with a
different payload type is safe: each handler gets its own typed frame slot, and
the source spelling is resolved only while that handler is active. Aggregate
payloads use the same generated clone/drop/mark hooks during extraction.

## Channels

`Channel<T>(capacity)` is a bounded FIFO channel. `send` and `receive` are
async operations, and `close` is idempotent:

```aura
async fun producer(channel: Channel<String>): Unit {
  channel.send("ready")
  channel.close()
}
```

`Channel<Unit>` uses a zero-sized runtime token, so it follows the same close
and transfer contract without allocating a payload.
Nullable aggregate elements retain their semantic nullable layers while using
the underlying pointer/value representation for channel ownership hooks; for
example, `Channel<Box?>.receive()` has the expected additional nullable result
boundary.

Channels require a positive capacity. Payloads cross an ownership boundary, so
borrowed `ref` values cannot be sent or retained by a channel. The bounded
runtime supports `Int`, `Bool`, `String`, classes, foreign handles, and the
generated clone/drop contracts for arrays, enums, value structs, interfaces,
and function values. Scheduler-owned `Task`, `TaskHandle`, and `Channel`
payloads use explicit retain/release wrappers.

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
