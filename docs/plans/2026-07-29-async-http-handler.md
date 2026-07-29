# Async HTTP Handler Completion Plan

Status: proposed
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
- Typed HTTP request/response accessors through a limited `std.http` bridge.
- Synchronous native route dispatch and native HTTP health fixtures.

Still incomplete:

- Aura-owned `Request` and `Response` values with a stable typed API.
- Compiler-generated handlers that can suspend and resume across arbitrary
  supported control flow.
- Transfer of request, response, socket, and body ownership into the handler
  task frame.
- End-to-end scheduler integration for accept, read, handler execution, and
  partial response writes.
- Async routing, timeout/disconnect cancellation, and a runnable Aura-level
  server example.

## Implementation checklist

### Contract and architecture

- [ ] C01 Freeze the public `std.http` request, response, route, and server API.
- [ ] C02 Freeze typed error conventions shared by `std.os`, `std.net`,
      `std.dns`, `std.http`, and protocol adapters.
- [ ] C03 Document copy, borrow, pin, ownership, GC-root, and destruction
      rules for every value that can cross `await`.
- [ ] C04 Define supported targets, capability checks, limits, timeouts, and
      compatibility policy for each protocol.
- [ ] C05 Add RFC/spec updates for decisions that affect language or runtime
      ABI behavior.

### Compiler and runtime

- [ ] R01 Complete general async state-machine lowering for handler control
      flow, multiple awaits, errors, cancellation, and cleanup.
- [ ] R02 Retain request/response/body/socket values in task frames safely.
- [ ] R03 Integrate accept, read, write, timeout, and cancellation waits with
      the scheduler and readiness poller.
- [ ] R04 Preserve partial-read and partial-write offsets across resumption.
- [ ] R05 Enforce borrow barriers across `await`, `spawn`, channel, and task
      outcome boundaries.
- [ ] R06 Verify exactly-once close/destroy under success, failure, cancel,
      disconnect, forced GC, and executor shutdown.
- [ ] R07 Add concurrent task limits, connection limits, body limits, and
      backpressure limits.

### Core stdlib

- [ ] S01 Complete shared `Result`/error types and platform error mapping.
- [ ] S02 Complete `std.task` and `std.time` task, timer, deadline, and
      cancellation APIs.
- [ ] S03 Complete `std.sync` mutex, rwlock, once, atomic, and lock-safety
      behavior.
- [ ] S04 Complete `std.bytes`/`std.stream` owned buffers and async reader/
      writer adapters.
- [ ] S05 Complete `std.os` process/environment and `std.fs` path/filesystem
      APIs.
- [ ] S06 Complete `std.net` TCP transport with typed async operations.
- [ ] S07 Complete `std.dns` resolution, timeout, cancellation, and address
      selection.
- [ ] S08 Complete `std.encoding` UTF-8, Base64, hex, and percent encoding.
- [ ] S09 Complete `std.url` and `std.mime` parsing and sanitization helpers.
- [ ] S10 Complete `std.json` value model, parser, serializer, typed mapping,
      limits, and diagnostics.
- [ ] S11 Complete `std.log`, `std.metrics`, and `std.signal` integration.
- [ ] S12 Complete `std.test` async, timer, network, and sanitizer fixtures.

### HTTP and protocol support

- [ ] H01 Implement typed `Request`/`Response` values and async routing.
- [ ] H02 Implement HTTP/1.1 keep-alive, streaming bodies, errors, and
      graceful shutdown through Aura APIs.
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

### Security and interoperability

- [ ] X01 Add parser fuzzing for HTTP/1.1, HTTP/2, HTTP/3, WebSocket, JSON,
      URL, MIME, and multipart inputs.
- [ ] X02 Add hostile-client tests for slowloris, oversized headers/bodies,
      decompression bombs, invalid frames, and connection exhaustion.
- [ ] X03 Run ASAN/UBSAN and forced-GC tests across every native resource path.
- [ ] X04 Run protocol conformance suites and verify ALPN negotiation,
      certificate policy, framing, and status/error mapping.
- [ ] X05 Audit secrets, private keys, logs, authorization data, and error
      messages for accidental exposure.

### Examples, docs, and release

- [ ] D01 Replace native-only health fixtures with a real Aura async HTTP
      server example.
- [ ] D02 Add examples for TLS, HTTP/2, HTTP/3, WebSocket, compression, and
      multipart upload/download.
- [ ] D03 Document local build/run commands, limits, target support, and
      troubleshooting for every example.
- [ ] D04 Run clean-host acceptance with the installed CLI and embedded stdlib.
- [ ] D05 Update release notes, roadmap, RFC status, and `agents/debts.md` for
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

| Package                                 | Required evidence                                                                        |
| --------------------------------------- | ---------------------------------------------------------------------------------------- |
| `std.os`                                | Environment, cwd, metadata, process status, and unsupported-target behavior              |
| `std.net`                               | Loopback TCP, concurrent clients, partial I/O, timeout, close, cancellation, sanitizer   |
| `std.dns`                               | Successful IPv4/IPv6 resolution, invalid host, timeout/cancel, deterministic errors      |
| `std.json`                              | Parse/stringify round trips, typed structs/enums, limits, malformed input diagnostics    |
| `std.http`                              | Typed request/response, routing, keep-alive, async handler await, 4xx/5xx mapping        |
| `std.task` / `std.time`                 | Repeatable task outcomes, cancellation, monotonic deadlines, timers                      |
| `std.stream` / `std.bytes`              | Owned buffers, partial I/O, async read/write, backpressure adapters                      |
| `std.encoding` / `std.url` / `std.mime` | Boundary-safe encoding, URL/query parsing, MIME and multipart metadata                   |
| `std.crypto` / `std.tls`                | Secure randomness, certificate/key handling, TLS, SNI, ALPN, cleanup                     |
| `std.sync`                              | Async-safe locks/atomics with cancellation and contention tests                          |
| `std.log` / `std.metrics`               | Structured request logs and non-blocking server telemetry                                |
| `std.signal` / `std.fs`                 | Graceful shutdown, path/filesystem operations, typed platform errors                     |
| `std.test`                              | Deterministic async, HTTP, timeout, cancellation, and sanitizer helpers                  |
| Integration                             | Aura HTTP server uses only `std.*`, performs real async I/O, and runs from installed CLI |

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
- Mutation-through-entry collection views or `Array<Interface>`.
- Stable public language/runtime ABI.
- Windows release support until a separate target and backend acceptance gate
  exists.

## Exit criteria

Mark this plan complete only when the public Aura API, compiler lowering,
runtime scheduler path, ownership rules, async routing, Aura example, and
acceptance matrix are all implemented and documented. If a capability remains
bounded or deferred, keep it explicitly listed here and in the alpha completion
matrix rather than claiming a complete async HTTP handler.
