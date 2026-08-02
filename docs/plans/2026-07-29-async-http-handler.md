# Async HTTP Handler Completion Plan

Status: in progress
Owner: Compiler Expert + Runtime & Integration
Related workstreams: G1-G5, A4-A8, IO3-IO5, H5-H8, RFC-007

## Goal

Allow an Aura application to define an HTTP handler that can suspend at
`await`, serve concurrent connections, preserve request state, and clean up
correctly on success, failure, timeout, client disconnect, cancellation, and
executor shutdown.

The completion target is a production-shaped HTTP stack on the currently
supported POSIX release targets, backed by a usable first-party networking and
platform stdlib surface. The initial implementation remains bounded, but the
plan includes HTTP/2, HTTP/3, TLS termination, WebSockets, compression, and
streaming multipart bodies.

## Current baseline

Already available in bounded form:

- HTTP/1.1 parser, response builder, keep-alive policy, request limits, and
  native connection lifecycle.
- Nonblocking POSIX TCP listener/stream primitives and bounded fd readiness.
- Runtime task frames, cancellation cleanup hooks, GC root retention, and
  bounded async I/O lowering.
- Typed HTTP request/response values through the `std.http` bridge, including
  request snapshots and task-owned response builders.
- Aura-level async routing and native HTTP health/shutdown fixtures.

Still incomplete:

- General async lowering beyond the currently covered bounded CFG and
  ownership families.
- Full typed task/error and stream abstractions for the broader stdlib.
- Extended protocols (TLS, HTTP/2, HTTP/3/QUIC, WebSockets, compression,
  multipart) and an HTTP client.
- Cross-platform and clean-installed-release acceptance for every protocol.

## Implementation checklist

### Contract and architecture

- [x] C01 Freeze the public `std.http` request, response, route, and server API.
- [x] C02 Freeze typed error conventions shared by `std.os`, `std.net`,
      `std.dns`, `std.http`, and protocol adapters.
- [x] C03 Document copy, borrow, pin, ownership, GC-root, and destruction
      rules for every value that can cross `await`.
- [x] C04 Define supported targets, capability checks, limits, timeouts, and
      compatibility policy for each protocol.
- [x] C05 Add RFC/spec updates for decisions that affect language or runtime
      ABI behavior.

Contract decision (2026-07-29): the first-class handler is
`Handler = (Request, Response) -> Task<Unit>`, with a task-owned mutable
`Response` builder rather than a returned response value. `Request` is an owned snapshot
and `Response` is valid only until the handler's terminal path. The public API
contains no native or `ForeignHandle` values. The frozen base target is bounded
HTTP/1.1 on Linux amd64 and macOS arm64; all extended protocols remain explicit
capability-gated follow-ons. RFC-007 records the limits, failure mapping,
ownership, cancellation, and exactly-once cleanup rules.

### Compiler and runtime

- [x] R01 Complete general async state-machine lowering for handler control
      flow, multiple awaits, errors, cancellation, and cleanup.

  Completed (2026-08-01): handler lowering now uses the general CFG state
  machine for branches, loops, repeated awaits, typed errors, cancellation,
  nested `finally`, async class methods, and scheduler-owned `Task`,
  `TaskHandle`, and `Channel` locals. Discarded `Unit` awaits in spawned
  handlers use a dedicated resumable poller with the same typed failure and
  cancellation cleanup contract. Handle-backed HTTP polling arms its cleanup
  hook immediately after pin acquisition, and the foreign pin is released by
  terminal frame cleanup. Codegen (259 tests) and the complete sanitizer
  manifest cover nested failure, suspension, cancellation, forced GC, and
  response backpressure. Generic open type declarations, unsupported spawn
  body shapes, and non-HTTP/1.1 protocols remain explicit separate limits.

  Progress: the general CFG now converts primitive `throw` statements reached
  after an `await` into owned task-frame errors rather than longjmping through
  a returned poller. Error text, source span, cancellation cleanup, and
  repeated joins are covered for the String path. Bounded single-await catches
  now decode owned `String`, `Int`, and `Bool` failures by tagged task-frame
  error type, including a non-Unit awaited value declaration whose success
  value is copied before the catch continuation. Finally-on-failure remains
  bounded to one awaited child.
  Multi-await catch regions now support primitive and array payload paths;
  nested failure cleanup remains open; bounded
  non-generic class throws without array fields
  now clone typed payloads into the task-frame error slot. Catch-free
  `try/finally` success paths now lower through explicit continuation states.
  Async class methods
  now route through the bounded async dispatch used by top-level functions,
  retaining a synthetic `this` frame slot and rebinding it in the covered CFG
  resume paths. Closed generic class monomorphs now receive the same lowering
  when the method body is independent of the class type parameter; richer
  generic payload substitution and fully general class-method CFG coverage
  still need additional method-aware lowering.

  Evidence: `parses_async_class_method_as_task_returning_method` verifies the
  `async fun method(): T` class syntax lowers to `Task<T>`, while
  `class_task_method_allows_await_and_checks_inner_result` covers sema's async
  context and inner-result checking. The codegen fixture
  `compiles_async_class_method_with_await` covers frame emission and rebinding
  `this` after one and two suspension points. The runtime fixture
  `builds_and_runs_async_class_method_branch_loop` additionally covers a
  method-aware general CFG with a loop, conditional, repeated awaits, and
  typed `this.value` access after suspension.
  `compiles_async_method_on_generic_class_mono` covers a closed `Box<Int>` async
  method frame and wrapper. `builds_and_runs_async_generic_class_method_returning_type_parameter`
  now exercises substituted `T` return values for `Int`, owned `String`,
  `Array<Int>`, nested `Node` class, and `Array<Node>` monomorphs after an
  await, including sanitizer cleanup of aggregate and nested-class results.
  Short nested class keys are normalized to package-qualified C symbols before
  method dispatch.
  `builds_and_runs_async_try_finally_after_await` covers the success path for
  `try/finally` around an awaited operation.
  `builds_and_runs_async_catch_after_await`,
  `builds_and_runs_async_non_unit_catch_after_await`,
  `builds_and_runs_async_primitive_catches_after_await`, and
  `builds_and_runs_async_class_method_catch_after_await` cover the bounded
  single-await `catch (String)` path, including owned error extraction from
  the child task and class-method frame lowering.
  `builds_and_runs_async_finally_after_await_failure` now proves that a failed
  awaited child propagates its owned error only after the `finally` body runs.
  `compiles_async_class_throw_after_await_with_owned_payload` covers typed
  class error cloning and type-name propagation. Multi-await try regions with
  primitive catches now lower through one shared catch continuation and are
  covered by `builds_and_runs_async_multi_await_catch_region`. Typed class
  catches now clone the child payload into an owned frame slot, survive forced
  GC, and are covered by `builds_and_runs_async_class_catch_after_await`.
  Same-name catches with changing types, nested class-field rooting, and nested
  failure cleanup remain open.

- [x] R02 Retain request/response/body/socket values in task frames safely.

  Evidence: generated HTTP request/response/body tasks pin their opaque
  handles across readiness waits and release them on every terminal path;
  socket/connection handles are retained by the async server bridge. Native
  coverage exercises partial body reads, streamed response writes, typed
  connection pins across `await`, cancellation, disconnect, forced GC, and
  executor shutdown.

- [x] R03 Integrate accept, read, write, timeout, and cancellation waits with
      the scheduler and readiness poller.
- [x] R04 Preserve partial-read and partial-write offsets across resumption.
- [x] R05 Enforce borrow barriers across `await`, `spawn`, channel, and task
      outcome boundaries.

  Evidence: sema rejects borrow-derived operands at `await`, `spawn`, channel
  create/send/receive/close, `join`, and `cancel`; it also rejects reference
  payloads in task and channel storage plus escaping return/assignment paths.
  `async_boundaries_reject_borrowed_values` covers the task-outcome and channel
  regressions.

- [x] R06 Verify exactly-once close/destroy under success, failure, cancel,
      disconnect, forced GC, and executor shutdown.

  Evidence: `runtime/tests/http_async.c` covers suspended success, handler
  failure, cancellation, peer disconnect, forced GC while a typed handle is
  pinned, timeout, keep-alive, and executor/server teardown; the native fixture
  passes under strict compilation and ASAN/UBSAN manifest execution.

- [x] R07 Add concurrent task limits, connection limits, body limits, and
      backpressure limits.

  Evidence: the executor enforces a configurable bounded live-task count
  (default 4096, hard cap 65536); HTTP servers enforce max active connections
  and max requests per connection; parser/reader and response streaming enforce
  bounded headers, bodies, aggregate buffers, and output. Native tests cover
  connection-limit rejection, task-limit rejection/recovery, oversized input,
  and POLLOUT backpressure resumption.

### Core stdlib

- [ ] S01 Complete shared `Result`/error types and platform error mapping.

  Progress: embedded `std.error` now provides a transport-neutral `ErrorKind`,
  owned `Error`, and generic `Outcome<T, E>` with import-safe success/failure
  constructors. The bounded HTTP client exposes typed `getResponseResult` and
  `postResponseResult` framing outcomes while preserving the raw compatibility
  APIs. `std.error.kindCode` now maps common native errno/status values into
  stable category IDs (invalid input, not found, permission, I/O, network,
  timeout, cancellation, protocol, limit, closed, unsupported, unknown), with
  native codegen coverage in the shared error fixture. Full transport-specific
  errors now expose an explicit `isRetryable` policy for transient I/O,
  network, and timeout outcomes. Full transport-specific error payloads and
  same-named `Result` unification remain open because the
  merged-package resolver currently cannot disambiguate duplicate generic enum
  names.

- [ ] S02 Complete `std.task` and `std.time` task, timer, deadline, and
      cancellation APIs.

  `std.task.joinTask` and `std.task.cancelTask` now expose the bounded existing
  lifecycle over `TaskHandle<T>` and typed task outcomes. `std.task.isCancelled`
  provides a cooperative cancellation query inside generated async frames.
  `std.time.sleep` is monotonic, and `std.time.Duration`/`sleepFor` provide a
  typed duration layer; absolute deadlines now use monotonic `Deadline`,
  `after`, and `sleepUntil` APIs. Timeout composition and parent/child
  structured cancellation remain open. `std.task.cancelAfter<T>` now installs
  a monotonic cancellation deadline on a live task handle; the scheduler wakes
  pending tasks at expiry and publishes cooperative cancellation.
  `std.task.linkCancellation<P,C>` now provides bounded parent-to-child
  cooperative cancellation for handles sharing one executor; frame teardown
  unlinks relationships deterministically. Full cancellation scopes,
  deadlines spanning multiple children, and graceful executor shutdown remain
  open.
  Evidence: `lowers_std_task_is_cancelled_inside_async_frame` checks the frame
  ABI lowering and native compilation; `corpus/std_task/lifecycle` checks the
  public package surface and linking API; the runtime cancellation test covers
  propagation and cleanup.
  `corpus/std_time/duration` checks the public package surface; the codegen
  timer fixture runs the real monotonic wait.

- [ ] S03 Complete `std.sync` mutex, rwlock, once, atomic, and lock-safety
      behavior.

  Progress: embedded `std.sync.AtomicInt` now provides sequentially consistent
  `load`, `store`, `fetchAdd`, and `compareExchange` operations backed by
  compiler atomics. Native codegen and `corpus/std_sync/atomic` cover the
  closed class ABI. The same bounded package now includes CAS-based,
  non-blocking `Mutex.tryLock`/`unlock` and one-shot `Once.tryEnter`; the
  native fixture and corpus exercise ownership-free state transitions. A
  bounded CAS-based `RwLock` now supports concurrent readers, exclusive
  writers, explicit read/write unlock, and state inspection. Blocking/async
  lock adapters, lock ordering, and broader atomic types remain open.

- [ ] S04 Complete `std.bytes`/`std.stream` owned buffers and async reader/
      writer adapters.

  Progress: embedded `std.bytes` now provides owned `copy`, `concat`, bounded
  `slice`, and byte-wise `equals` operations with native allocation and null
  bounds failures. `builds_and_runs_std_bytes_owned_operations` and
  `corpus/std_bytes/owned` cover codegen, package embedding, and CLI checks.
  Embedded `std.stream.Reader`/`Writer` classes now wrap owned TCP streams with
  async `read`/`write` methods and idempotent close operations; the
  `corpus/std_stream/adapters` check and native build cover class-method async
  lowering. `std.bytes.Buffer` now provides owned `Array<Int>` storage with
  byte-range validation, nullable indexing, and deep cloning; the
  `corpus/std_bytes/buffer` fixture covers its native class ABI. Zero-copy
  views, raw descriptor-backed buffers, and richer stream error/backpressure
  adapters remain open.

- [ ] S05 Complete `std.os` process/environment and `std.fs` path/filesystem
      APIs.

  Progress: embedded `std.fs` now provides bounded portable `join`,
  `basename`, `dirname`, `extension`, and `isAbsolute` helpers with native
  owned results. `builds_and_runs_std_fs_path_helpers` and
  `corpus/std_fs/paths` cover the ABI and clean CLI path. Metadata, directory,
  permissions, process, and typed platform-error APIs remain open.
  Embedded `std.os` additionally provides bounded environment lookup/mutation,
  cwd, pid, and platform identification; the native fixture and
  `corpus/std_os/process` cover the process-facing ABI. Typed environment
  wrappers now return shared `Outcome` values for missing variables and failed
  updates. Process spawning, signals, permissions, and typed process errors
  remain open. `std.fs` now also
  exposes bounded `isDirectory` and stable `fileMode` metadata queries backed
  by `stat`, covered by the native path fixture and `corpus/std_fs/paths`.
  `std.fs.permissions` now exposes the low nine POSIX permission bits with a
  stable zero-on-error fallback, also covered by that fixture. The same
  package now exposes second-resolution `modifiedMillis` epoch timestamps with
  a `-1` error sentinel. `listNames` adds a newline-delimited directory
  snapshot capped at 64 KiB and returns null on unsupported/error paths.
  `isSymlink` adds an explicit non-following link check (`lstat` on POSIX),
  covered by the native fixture and `corpus/std_fs/paths`. Typed
  `readTextResult`/`writeTextResult` wrappers now map soft file failures to the
  shared `std.error.Outcome` contract. Directory iteration, process APIs, and
  richer platform-error mapping remain open.

- [ ] S06 Complete `std.net` TCP transport with typed async operations.

  Progress: `std.net` now provides non-throwing `readStreamResult` and
  `writeStreamResult` wrappers returning `std.error.Outcome`, while the
  existing readiness-driven stream operations remain available for
  compatibility. `corpus/std_net/typed` verifies the shared outcome types and
  native build path. Address-list APIs, richer timeout/cancellation payloads,
  and cross-platform transport errors remain open. String success payloads now
  clone and deep-clean through the owned `OutcomeOk` constructor; generic enum
  class-payload rooting remains tracked separately in `ERROR-002`.

- [ ] S07 Complete `std.dns` resolution, timeout, cancellation, and address
      selection.

  Progress: embedded `std.dns.resolveHost(host, preferIpv6)` performs a
  bounded numeric IPv4/IPv6 lookup, prefers the requested family, falls back
  to the other family, and returns an owned nullable address. Native codegen
  and `corpus/std_dns/resolve` cover the ABI and literal-address path. Async
  resolver cancellation, explicit timeout enforcement, and service-name
  lookup remain open. `resolveHostList` now returns a preference-ordered,
  newline-delimited numeric address snapshot capped at 64 KiB, while
  `resolveHostResult` wraps lookup failure in the shared `std.error.Outcome`
  network error type.

- [x] S08 Complete `std.encoding` UTF-8, Base64, hex, and percent encoding.

  The embedded `std.encoding` package exposes UTF-8 validation, RFC 4648
  Base64, lowercase hex, and RFC 3986 percent encode/decode functions. Native
  implementations are bounded by the input string size, reject malformed
  escapes/alphabets and decoded NUL bytes, and return nullable results for
  invalid decodes. `builds_and_runs_std_encoding_round_trips` and
  `corpus/std_encoding/roundtrip` cover native execution, package embedding,
  and the clean CLI build path.

- [ ] S09 Complete `std.url` and `std.mime` parsing and sanitization helpers.

  Progress: embedded bounded packages now validate HTTP origin-form targets,
  extract path/query components, recognize bounded absolute authorities,
  extract userinfo-safe host/port components and exact raw query values,
  validate media types with parameters, sanitize upload filenames, and extract
  sanitized MIME disposition filenames. URL-level `encodeComponent` and
  `decodeComponent` now reuse the strict percent codec and reject malformed
  escapes/NULs. `normalizePath` now removes bounded dot segments while
  preserving the origin-form root and trailing slash. Native fixtures and
  `corpus/std_url_mime/sanitize` cover the ABI and clean CLI path. Full RFC
  URL normalization and multipart metadata remain open.

- [ ] S10 Complete `std.json` value model, parser, serializer, typed mapping,
      limits, and diagnostics.

  Progress: embedded `std.json` now validates complete UTF-8 JSON values with
  bounded 64-level nesting, strings, arrays, objects, literals, and strict
  number grammar; `escapeString` emits owned JSON string literals. Native
  codegen and `corpus/std_json/basic` cover valid/invalid framing and escapes.
  `std.json.Value` now retains validated JSON text with owned `raw` and
  `serialize` accessors; `parse` returns null for invalid input and
  `corpus/std_json/value` covers the model. `Value.kind` and root-type
  predicates now provide bounded object/array/string/number/bool/null
  navigation. `errorOffset` reports the first invalid byte (or `-1` for
  complete JSON). Member/array traversal, serializer ordering,
  typed mapping, and configurable limits remain open.

- [ ] S11 Complete `std.log`, `std.metrics`, and `std.signal` integration.

  Progress: embedded `std.log` now exposes bounded debug/info/warn/error text
  levels with deterministic stderr prefixes and flush behavior. Native codegen
  and `corpus/std_log/basic` cover the package surface. Structured key/value
  info/error helpers now render deterministic fields through the existing
  sinks. `setMinLevel`/`minLevel` now provide a bounded process-local level
  filter; configurable sinks and OS signal integration remain open.
  Embedded `std.metrics.Counter`
  now provides sequentially consistent add/increment/get/reset operations;
  native codegen and `corpus/std_metrics/counter` cover the counter ABI.
  `Counter.prometheus(name)` now emits one bounded text exposition sample.
  Cross-process aggregation and richer export formats remain open. Embedded
  `std.signal` now installs SIGINT/SIGTERM handlers and exposes a clearable
  shutdown flag; native codegen and `corpus/std_signal/shutdown` cover the
  supported-target state path. The generated `std.http.serve` loop now observes
  the flag, closes the listener, stops accepting, and waits for tracked
  connection tasks to reach terminal state before completing. Unsupported-target
  typed errors are still open; `runtime/tests/signal_shutdown.c` now proves
  SIGINT/SIGTERM delivery and clear/re-arm behavior in the sanitizer matrix.

- [ ] S12 Complete `std.test` async, timer, network, and sanitizer fixtures.

  Progress: embedded `std.test` now provides deterministic Bool/Int/String
  assertion helpers backed by the native failure diagnostics. Native codegen,
  `corpus/std_test/assertions`, and the existing `@test` harness cover the
  synchronous assertion path. `corpus/std_test/async` now runs assertions
  after a real `std.time.sleep` suspension and prints its completion marker;
  async network helpers, fixture isolation, and broader sanitizer orchestration
  remain open.

### HTTP and protocol support

- [x] H01 Implement typed `Request`/`Response` values and async routing.
- [x] H02 Implement HTTP/1.1 keep-alive, streaming bodies, errors, and
      graceful shutdown through Aura APIs.

  Bounded keep-alive, timeout/500 error mapping, partial writes, and listener
  shutdown are covered by `runtime/tests/http_async.c` and the shutdown corpus
  fixture. `scripts/http-aura-smoke.sh` launches the Aura example on an
  isolated port and verifies GET `/health` (200), unknown target (404), and
  POST `/health` (405), plus 16 concurrent GET clients, after the handler
  suspends twice. The runtime also decodes bounded inbound chunked request
  bodies into owned snapshots; the Aura `/stream` example verifies that payload
  survives handler suspension. Content-Length and chunked request streaming,
  plus chunked response streaming, are available. Full request snapshots retain
  validated chunked trailer fields; streaming readers validate and consume
  trailers before publishing EOF.

  Streaming contract (2026-07-29, implemented for Content-Length and chunked):
  `Request` exposes one `RequestBody` reader for a Content-Length- or
  chunked-framed request.
  `await readChunk(capacity)` returns an owned, non-empty String of at most
  `min(capacity, 16 KiB)`, or `""` exactly once EOF is reached. The reader is
  valid only while its handler task is alive, only one read may be pending,
  and the connection cannot parse its next keep-alive request until EOF. A
  handler that returns/cancels before EOF forces `Connection: close`, avoiding
  request-boundary desynchronization. Disconnection, cancellation, and a body
  read timeout terminate the reader and close the connection; buffered
  response output is discarded. Chunked trailer fields are validated and
  consumed; full request snapshots expose them through the existing header
  lookup API, while streaming readers keep them internal to the framing
  boundary.

  Foundation progress: the native parser now has an internal header-first mode
  that validates and owns request metadata while reporting the exact header
  boundary plus Content-Length/chunked framing without consuming body bytes.
  `runtime/tests/http_parser.c` verifies partial Content-Length and chunked
  inputs. Task handlers for non-empty Content-Length and chunked requests now
  enter before the full body arrives; synchronous handlers retain the proven
  snapshot path.

  The native Content-Length reader core now consumes only the declared body
  bytes from a connection-owned unread buffer or socket and leaves pipelined
  bytes untouched. `runtime/tests/http_async.c` proves a task handler parks on
  a partial body, resumes at EOF, and parses the following pipelined request.
  Returning before EOF forces `Connection: close`. This reader is still a
  public Aura `RequestBody.readChunk` bridge: the parser accepts async class
  methods as `Task<T>` methods and compiler lowering pins the request only for
  the read task, caps each owned chunk at 16 KiB, and resumes with the reader's
  body deadline. `corpus/std_http/stream_body` builds the method call and the
  `/stream` smoke route executes it. A terminal reader task releases its
  exclusive claim immediately, so sequential calls do not depend on executor
  frame reclamation. `Response.writeChunk(body)` pins the response and
  connection across partial writes, commits chunked headers once, emits each
  owned chunk, and appends the terminal chunk after handler completion.
  `runtime/tests/http_response.c` covers framing and post-commit mutation;
  `/stream-response` smoke validates two awaited chunks as `onetwo`.
  The streaming reader validates trailer field names/values, rejects framing
  fields, consumes the complete trailer section, and only then returns EOF.

- [ ] H03 Implement TLS termination, certificate loading, SNI, ALPN, reload,
      key cleanup, and handshake errors.
- [ ] H04 Implement HTTP/2 framing, HPACK, stream lifecycle, multiplexing,
      flow control, reset, and graceful GOAWAY.
- [ ] H05 Implement HTTP/3 over the selected QUIC backend, including TLS 1.3,
      QPACK, stream lifecycle, timeout, and unsupported-target behavior.
- [ ] H06 Implement WebSocket upgrade, frames, fragmentation, control frames,
      masking, UTF-8 validation, close, and backpressure.
- [ ] H07 Implement streaming gzip/deflate compression with negotiation,
      flush, cancellation, and decompression-ratio limits.
- [ ] H08 Implement streaming multipart parsing, part limits, boundary
      handling, cancellation, and direct file sinks.
- [ ] H09 Add HTTP client APIs for HTTP/1.1 and the supported HTTP/2/HTTP/3
      transports.

  Bounded HTTP/1.1 loopback `std.http.get(port, target)` and
  `std.http.post(port, target, body)` now run over `std.net`; both collect
  readiness-driven TCP reads through EOF with a 64 KiB aggregate limit. POST
  writes an exact `Content-Length` and deliberately closes after one response.
  `corpus/std_http/client`, `corpus/std_http/client_post`, and
  `scripts/http-aura-smoke.sh` prove standalone Aura clients obtain the Aura
  server's `200 OK` health and echo responses.
  `getResponse` and `postResponse` additionally parse the bounded status line
  and body into `ClientResponse`; invalid framing yields status zero until
  typed client errors are available. `getResponseResult` and
  `postResponseResult` now return the shared `std.error.Outcome` with a
  protocol `Error` for invalid framing. Custom request headers, request/response
  streaming, typed errors, TLS, HTTP/2, and HTTP/3 remain open, so this row is
  intentionally unchecked.

### Security and interoperability

- [ ] X01 Add parser fuzzing for HTTP/1.1, HTTP/2, HTTP/3, WebSocket, JSON,
      URL, MIME, and multipart inputs.

  Progress: `runtime/tests/http_parser_fuzz.c` mutates deterministic
  Content-Length, chunked, and keep-alive request seeds, while
  `runtime/tests/stdlib_parser_fuzz.c` mutates bounded JSON, percent, URL,
  and MIME inputs under ASAN/UBSAN.
  HTTP/2, HTTP/3, WebSocket, and multipart parser seeds remain open with those
  protocol implementations.

- [ ] X02 Add hostile-client tests for slowloris, oversized headers/bodies,
      decompression bombs, invalid frames, and connection exhaustion.

  Progress: the existing HTTP hardening, parser-fuzz seed, async disconnect,
  timeout, active-connection-limit, oversized-header/body, and malformed
  framing fixtures run in the sanitizer matrix. Slowloris/decompression-bomb
  coverage and extended-protocol invalid-frame tests remain open.

- [ ] X03 Run ASAN/UBSAN and forced-GC tests across every native resource path.

  Evidence: `bash scripts/sanitizer-smoke.sh` passed the complete current
  `runtime/tests/sanitizer-seeds.tsv` matrix, including HTTP parser fuzz,
  HTTP hardening, HTTP async lifecycle, HTTP health, async I/O, task
  cancellation/GC, and FFI paths. Full extended-protocol and cross-target
  coverage remains open.

- [ ] X04 Run protocol conformance suites and verify ALPN negotiation,
      certificate policy, framing, and status/error mapping.
- [ ] X05 Audit secrets, private keys, logs, authorization data, and error
      messages for accidental exposure.

### Examples, docs, and release

- [x] D01 Replace native-only health fixtures with a real Aura async HTTP
      server example.
- [ ] D02 Add examples for TLS, HTTP/2, HTTP/3, WebSocket, compression, and
      multipart upload/download.
- [x] D03 Document local build/run commands, limits, target support, and
      troubleshooting for every example.
- [x] D04 Run clean-host acceptance with the installed CLI and embedded stdlib.

  Evidence: an offline release install under `/private/tmp/aura-install`
  passed `aura check`, `aura build`, and loopback GET/404/405 smoke for the
  Aura health server.

- [x] D05 Update release notes, roadmap, RFC status, and `agents/debts.md` for
      every deferred or bounded capability.
- [ ] D06 Do not mark the feature complete until all required acceptance rows
      below have reproducible evidence.

## Work plan

### 1. Freeze the public contract

- Define the supported HTTP/1.1 subset: methods, origin-form targets, header
  limits, body limits, status codes, keep-alive, timeout, and shutdown.
- Define the Aura API, for example:

  ```aura
  class Request {
    val method: String
    val target: String
    val version: String
    val headers: Map<String, String>
    val body: String
  }

  class Response {
    fun status(code: Int): Bool
    fun header(name: String, value: String): Bool
    fun body(content: String): Bool
    fun close(): Bool
  }

  async fun serve(listener: ForeignHandle<Int>,
                  handler: (Request) -> Task<Response>): Unit
  ```

- Decide whether handlers return `Response`, mutate a response builder, or
  use a `Result<Response, Error>` outcome. Record the choice in the relevant
  RFC before exposing it in `std.http`.
- Specify which values are copied, borrowed, pinned, or owned across `await`.

### 2. Complete async lowering for handler-shaped code

- Lower multiple awaits in handlers into deterministic resume states.
- Cover conditionals, loops, `match`, nested calls, and `try/catch/finally`.
- Retain live request data, response buffers, task children, and typed socket
  handles in the frame.
- Reject borrowed values that cross `await`, `spawn`, or channel boundaries.
- Add cleanup edges for normal return, thrown error, cancellation, timeout,
  peer close, and executor shutdown.

### 3. Integrate HTTP with the scheduler

- Accept connections without blocking worker tasks.
- Suspend request reads on fd readiness and resume after partial reads.
- Spawn one bounded handler task per request/connection according to the
  chosen keep-alive policy.
- Suspend response writes on `POLLOUT`, preserving the write offset.
- Apply connection and handler limits so a slow client cannot exhaust the
  executor or memory.
- Ensure cancellation removes readiness registrations before frame teardown.

### 4. Implement typed request/response ownership

- Replace the current accessor-only bridge with compiler-backed `Request` and
  `Response` representations or an explicitly documented equivalent.
- Keep request body and headers valid for the handler lifetime only.
- Make response headers/status/body mutations validate input and report
  failure without leaking native buffers.
- Pin native handles only while needed and release them exactly once.
- Define maximum body/header sizes and behavior for malformed or oversized
  requests.

### 5. Add async routing and errors

- Route by method and target without exposing parser or socket internals.
- Return 404 for unknown targets and 405 for method mismatches.
- Map handler failures to a bounded 500 response.
- Define timeout, client disconnect, cancellation, and shutdown outcomes.
- Prevent a handler from writing after its connection has been closed.

### 6. Prove the clean user journey

- Add a real Aura example under `examples/http-health-aura` that starts a
  listener and serves `/health` through an async Aura handler.
- Add a handler that performs at least one real `await` before responding.
- Add CLI commands and documentation for build, run, and local curl smoke.
- Verify installed CLI behavior with the embedded runtime and std packages.

## Standard library completion track

The HTTP handler cannot be considered complete while applications still need
ad-hoc native bridges for networking, HTTP values, DNS, platform operations,
or JSON. The following packages are part of the same delivery plan and should
share the same `Result`/error, ownership, cancellation, and cross-target rules.

### `std.net`

Provide the transport layer used by servers and clients:

- `TcpListener`, `TcpStream`, connect, accept, read, write, flush, close, and
  timeout APIs with typed status/error results.
- Async variants that suspend on readiness and preserve buffers across short
  reads, short writes, cancellation, and peer disconnect.
- Explicit ownership for descriptors and handles; close must be idempotent and
  destroy must release exactly once.
- POSIX capability detection and a documented unsupported-target result.
- UDP and Unix-domain sockets remain a follow-up unless required by the HTTP
  contract.

### `std.http`

Provide typed HTTP values and the async server/client boundary:

- `Request`, `Response`, headers, query/target access, bounded body handling,
  status and response construction.
- HTTP/1.1 client request support over `std.net` for the bounded contract.
- Async route registration and handler execution with `await` support.
- Keep-alive, timeout, cancellation, disconnect, partial writes, and bounded
  backpressure behavior exposed through documented APIs.
- 400, 404, 405, 408, 413, 500, and transport failure mapping.
- No hidden raw FFI handles or borrowed views escaping the handler lifetime.

### `std.dns`

Add hostname resolution without blocking scheduler workers:

- `resolve(host, service)` returning typed address records.
- Async resolution with cancellation and timeout behavior.
- IPv4/IPv6 result representation and deterministic address ordering policy.
- Resolution errors mapped to typed `DnsError` values.
- Integration with `std.net.connect` and a bounded resolver cache policy, if
  caching is introduced.
- A synchronous convenience API may exist for scripts, but must be explicit
  about blocking behavior.

### `std.os`

Expose the minimum portable process and host surface needed by CLIs and
servers, without leaking platform-specific runtime internals:

- Environment lookup/set/unset, current directory, process ID, and platform
  information.
- Path and file metadata helpers that complement `std.io`.
- Process spawning with owned stdin/stdout/stderr handles, exit status, and
  explicit wait/cancellation semantics.
- Signal/shutdown hooks where supported, with unsupported-target results.
- Consistent permission, not-found, invalid-input, and interrupted-operation
  errors.
- No unrestricted unsafe syscall API in the core package.

### `std.json`

Add a bounded, allocation-safe JSON implementation for configuration, HTTP
payloads, and CLI data:

- JSON value model for null, bool, number, string, array, and object.
- Parser with byte/line/column diagnostics, configurable depth and size
  limits, and rejection of malformed UTF-8 or invalid JSON numbers.
- Serializer with deterministic object-key ordering or an explicit ordering
  contract, correct escaping, and configurable compact/pretty output.
- Typed decode/encode for Aura structs, enums, nullable values, arrays, and
  maps; manual serializer hooks may be the MVP fallback.
- No implicit lossy numeric conversion; overflow and unsupported shapes must
  return typed errors.
- Tests for duplicate keys, escapes, unicode, nesting limits, large payloads,
  round trips, and HTTP request/response bodies.

### Supporting packages

The protocol packages also require these shared stdlib surfaces:

#### `std.task`

- `Task<T>`, `TaskHandle<T>`, `spawn`, `join`, `cancel`, and repeatable typed
  outcomes.
- `sleep`, timeout/deadline helpers, structured cancellation, and graceful
  executor shutdown.
- Explicit rules for task ownership, GC roots, error propagation, and values
  crossing `await` or channel boundaries.

#### `std.time`

- Monotonic `Instant`, `Duration`, deadlines, timeout conversion, and timer
  sleep that does not depend on wall-clock changes.
- Wall-clock timestamps and formatting for logs and HTTP date headers.

#### `std.sync`

- `Mutex`, `RwLock`, `Once`, and atomics for runtime-integrated shared state.
- Async-safe locking rules, cancellation behavior, lock ordering guidance,
  and deadlock-focused tests.

#### `std.stream` / `std.bytes`

- Owned byte buffers, bounded readers/writers, buffering, `read_exact`,
  `write_all`, flush, and async backpressure-aware streams.
- Safe adapters between files, TCP/TLS/QUIC streams, compression, multipart,
  and HTTP bodies without forcing full payloads into `String`.

#### `std.encoding`

- UTF-8 validation, Base64, hex, and percent encoding/decoding.
- Explicit malformed-input and size-limit errors for protocol boundaries.

#### `std.crypto` / `std.tls`

- Secure random bytes, SHA-256, HMAC, certificate/key loading, TLS policy,
  ALPN, SNI, and private-key zeroization at native boundaries.
- Keep cryptographic state out of ordinary Aura strings and return typed
  handshake and certificate errors.

#### `std.url` / `std.mime`

- URL parsing, origin-form targets, query parameters, authority, and safe
  percent encoding.
- MIME type parsing, content disposition, multipart boundaries, and filename
  sanitization.

#### `std.log`

- Structured levels, fields, request/connection IDs, timestamps, and output
  sinks that do not block async workers unexpectedly.
- Redaction hooks for authorization headers, cookies, private paths, and
  sensitive error data.

#### `std.signal`

- SIGINT/SIGTERM integration for graceful listener drain and task shutdown,
  with typed unsupported-target behavior.

#### `std.metrics`

- Counters, gauges, and latency histograms for connections, requests, active
  tasks, bytes, errors, cancellations, and backpressure.
- A non-blocking export interface suitable for a later ecosystem metrics
  package.

#### `std.test`

- Async test execution, deterministic timers, test HTTP clients, loopback
  fixtures, cancellation tests, and sanitizer-friendly integration helpers.

#### `std.fs`

- Path, directory, metadata, permissions, and filesystem-specific operations.
- Keep filesystem concerns separate from process/environment APIs in `std.os`;
  both packages must share the same typed error conventions.

## Stdlib dependency order

```text
std.os + std.net
        |
        +--> std.task + std.time + std.stream
        |                         |
        +--> std.tls -------------+--> HTTP/1.1 + HTTP/2
        |                              |
        |                              +--> WebSocket (TCP/TLS)
        |
        +--> QUIC + TLS 1.3 --> HTTP/3 --> WebSocket extended CONNECT
        |
        +--> std.encoding + std.url + std.mime
        |                         |
        +--> streaming body API --> compression --> multipart streaming
        |
        +--> std.log + std.metrics + std.signal + std.test
std.io + std.collections + std.json ----> application payloads
```

Recommended implementation order:

1. Stabilize shared errors, byte buffers, ownership, and `std.os` primitives.
2. Complete synchronous and async `std.net` on the supported POSIX targets.
3. Add `std.dns` and connect it to hostname-based networking.
4. Implement `std.json` value parsing/serialization and typed conversions.
5. Build typed `std.http` request/response/client/server APIs on top of the
   completed transport and JSON layers.
6. Add logging, metrics, signal handling, deterministic async tests, and
   filesystem/path helpers required by the examples.
7. Replace the native health fixture with an Aura async HTTP example and run
   the complete matrix below.

## Standard library acceptance criteria

| Package                                 | Required evidence                                                                            |
| --------------------------------------- | -------------------------------------------------------------------------------------------- |
| `std.os`                                | Environment, cwd, metadata, process status, and unsupported-target behavior                  |
| `std.net`                               | Endpoint-aware TCP, concurrent clients, partial I/O, timeout, close, cancellation, sanitizer |
| `std.dns`                               | Successful IPv4/IPv6 resolution, invalid host, timeout/cancel, deterministic errors          |
| `std.json`                              | Parse/stringify round trips, typed structs/enums, limits, malformed input diagnostics        |
| `std.http`                              | Typed request/response, routing, keep-alive, async handler await, 4xx/5xx mapping            |
| `std.task` / `std.time`                 | Repeatable task outcomes, cancellation, monotonic deadlines, timers                          |
| `std.stream` / `std.bytes`              | Owned buffers, partial I/O, async read/write, backpressure adapters                          |
| `std.encoding` / `std.url` / `std.mime` | Boundary-safe encoding, URL/query parsing, MIME and multipart metadata                       |
| `std.crypto` / `std.tls`                | Secure randomness, certificate/key handling, TLS, SNI, ALPN, cleanup                         |
| `std.sync`                              | Async-safe locks/atomics with cancellation and contention tests                              |
| `std.log` / `std.metrics`               | Structured request logs and non-blocking server telemetry                                    |
| `std.signal` / `std.fs`                 | Graceful shutdown, path/filesystem operations, typed platform errors                         |
| `std.test`                              | Deterministic async, HTTP, timeout, cancellation, and sanitizer helpers                      |
| Integration                             | Aura HTTP server uses only `std.*`, performs real async I/O, and runs from installed CLI     |

## Extended protocol track

These features extend the HTTP/1.1 milestone. They must reuse the same task,
ownership, cancellation, and backpressure contracts instead of introducing a
separate blocking server implementation.

### TLS termination

- Add `std.tls` or a runtime-backed TLS layer with TLS 1.2/1.3 policy, secure
  defaults, certificate chains, private keys, SNI, ALPN, and certificate
  reload.
- Keep private key material outside Aura GC-visible strings where possible;
  define explicit ownership and zeroization rules for native key storage.
- Map handshake, certificate, protocol, timeout, and peer-close failures to
  typed errors.
- Support TLS termination before HTTP/1.1 and HTTP/2; HTTP/3 uses QUIC's
  integrated TLS 1.3 handshake.
- Test expired certificates, unknown SNI, client abort during handshake,
  renegotiation rejection, protocol selection, and forced shutdown.

### HTTP/2

- Implement connection preface, frame parsing/serialization, stream IDs,
  stream lifecycle, SETTINGS, PING, GOAWAY, and RST_STREAM.
- Implement HPACK with bounded dynamic-table memory and header-list limits.
- Add per-connection and per-stream flow control, including response
  backpressure and cancellation of individual streams.
- Map multiple HTTP/2 streams onto independent Aura handler tasks while
  preserving connection-level cleanup.
- Negotiate `h2` through ALPN and reject protocol downgrades that violate the
  configured policy.
- Test multiplexing, header compression limits, stream reset, flow-control
  stalls, graceful drain, and TLS integration.

### HTTP/3

- Add a QUIC transport abstraction or a carefully bounded native backend with
  explicit ownership at the FFI boundary.
- Implement QUIC connection setup, stream mapping, stream cancellation,
  connection migration policy, idle timeout, and graceful close.
- Implement HTTP/3 control streams, request/response streams, SETTINGS,
  QPACK, and bounded header-block handling.
- Integrate TLS 1.3 and ALPN (`h3`) without exposing raw crypto state to Aura.
- Define platform/backend support separately from TCP; HTTP/3 must fail with a
  typed unsupported result where the QUIC backend is unavailable.
- Test packet loss/reordering through the selected backend's test hooks,
  stream reset, idle timeout, migration policy, QPACK limits, and shutdown.

### WebSockets

- Implement HTTP upgrade validation for HTTP/1.1 and the corresponding
  extended CONNECT path if HTTP/2/3 WebSockets are supported.
- Provide typed `WebSocket` operations for accept, receive, send, ping, pong,
  close, message size limits, and close codes.
- Support fragmented messages and control-frame interleaving without exposing
  frame-buffer aliases across `await`.
- Apply bounded backpressure to sends and cancellation-safe cleanup to pending
  receives.
- Enforce client masking rules, origin/configuration policy, UTF-8 validation
  for text frames, and protocol-error handling.
- Test fragmented text/binary messages, ping during fragmentation, slow peers,
  invalid masks, oversized messages, disconnect, and graceful close.

### Compression

- Provide response compression negotiation for `gzip` and `deflate`; add
  Brotli only behind an explicit capability if the runtime dependency is
  available.
- Add streaming encoder/decoder interfaces that preserve state across
  `await`, flush, cancellation, and partial writes.
- Enforce decompressed-size, compression-ratio, CPU/time, and output-buffer
  limits to prevent resource-exhaustion attacks.
- Set and validate `Content-Encoding`, `Accept-Encoding`, `Vary`, and entity
  length/framing rules correctly.
- Test incompressible data, empty bodies, flush boundaries, malformed input,
  decompression limits, cancellation, and backpressure.

### Streaming multipart bodies

- Add a streaming multipart parser over an async request body rather than
  loading the complete payload into memory.
- Parse boundaries, headers, content disposition, filenames, and fields while
  preserving chunk boundaries and partial delimiter matches.
- Expose a bounded part stream with explicit consumption, cancellation, and
  cleanup semantics; never retain a borrowed socket buffer after `await`.
- Enforce total body, per-part, header, part-count, and nesting limits.
- Support streaming uploads to `std.io`/`std.os` destinations without creating
  unbounded Aura strings.
- Test split boundaries, empty parts, quoted parameters, malformed headers,
  premature EOF, oversized parts, cancellation, and slow uploads.

## Extended protocol dependency order

```text
std.os + std.net
        |
        +--> std.tls ---------> HTTP/1.1 + HTTP/2
        |                              |
        |                              +--> WebSocket (TCP/TLS)
        |
        +--> QUIC + TLS 1.3 --> HTTP/3 --> WebSocket extended CONNECT
        |
        +--> streaming body API --> compression --> multipart streaming
```

Recommended order after the base stdlib track:

1. Finish typed streaming byte sources/sinks and cancellation-safe buffers.
2. Implement TLS termination and ALPN for TCP connections.
3. Add HTTP/2 framing, HPACK, multiplexed handlers, and flow control.
4. Add WebSocket upgrade and message streaming over HTTP/1.1, then extend to
   HTTP/2/3 only when the corresponding protocol contract is stable.
5. Add streaming compression with security limits.
6. Add streaming multipart parsing and direct file sinks.
7. Add QUIC/TLS 1.3 and HTTP/3, including QPACK and HTTP/3 WebSocket support.
8. Run the complete cross-protocol acceptance matrix on clean installations.

## Extended protocol acceptance criteria

| Feature         | Required evidence                                                                     |
| --------------- | ------------------------------------------------------------------------------------- |
| TLS termination | TLS 1.2/1.3 policy, certificate/SNI/ALPN, reload, handshake failure cleanup           |
| HTTP/2          | Multiplexing, HPACK limits, stream reset, flow control, graceful GOAWAY               |
| HTTP/3          | QUIC backend, TLS 1.3, QPACK, stream reset, idle timeout, unsupported-target behavior |
| WebSockets      | Upgrade, fragmented messages, control frames, masking, ping/pong, close, backpressure |
| Compression     | gzip/deflate negotiation, streaming flush, malformed input, decompression limits      |
| Multipart       | Incremental parser, split boundaries, limits, cancellation, direct file streaming     |
| Security        | ASAN/UBSAN, fuzzing, resource limits, key cleanup, hostile-client tests               |
| Compatibility   | HTTP/1.1, HTTP/2, and HTTP/3 protocol conformance tests on supported targets          |
| Integration     | Aura async handlers use the same typed API across protocols and preserve task cleanup |

## Acceptance matrix

The feature is complete only when all cases pass on Linux amd64 and macOS
arm64, with sanitizer coverage where applicable:

| Area          | Required evidence                                           |
| ------------- | ----------------------------------------------------------- |
| Basic request | GET `/health` returns 200 with the expected body            |
| Routing       | 404, 405, malformed request, and oversized request          |
| Suspension    | Handler awaits once and multiple times before responding    |
| Control flow  | Await inside conditional, loop, `match`, and error path     |
| Concurrency   | Many simultaneous clients remain responsive                 |
| Keep-alive    | Multiple requests on one connection work correctly          |
| Partial I/O   | Short reads and short writes preserve byte order            |
| Backpressure  | Slow readers do not block unrelated connections             |
| Failure       | Handler error produces one bounded 500 response             |
| Cancellation  | Timeout, client disconnect, explicit cancel, and shutdown   |
| Memory        | Forced GC, ASAN/UBSAN, and exactly-once resource cleanup    |
| Release       | Clean installed-release run without repository-only helpers |

## Non-goals for this milestone

- Arbitrary live request/response views that outlive their handler task.
- Stable public language/runtime ABI.
- Windows release support until a separate target and backend acceptance gate
  exists.

## Exit criteria

Mark this plan complete only when the public Aura API, compiler lowering,
runtime scheduler path, ownership rules, async routing, Aura example, and
acceptance matrix are all implemented and documented. If a capability remains
bounded or deferred, keep it explicitly listed here and in the alpha completion
matrix rather than claiming a complete async HTTP handler.
