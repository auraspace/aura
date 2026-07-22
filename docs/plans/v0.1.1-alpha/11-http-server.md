# Workstream 11: Minimal HTTP Server

Owner: Runtime + Stdlib + Tooling. Scope: 8 tasks.

The alpha target is a runnable HTTP/1.1 server built on async TCP primitives.
It is a small core server API, not a full web framework: ORM, middleware
ecosystem, templating, WebSocket, HTTP/2, and TLS are excluded unless the
contract matrix explicitly adds them.

## H1. HTTP server contract

**Objective:** Freeze behavior before parser and runtime work begins.
**Contract (frozen subset):** HTTP/1.1 only, with origin-form request targets
(`/path` plus optional query), `GET`, `HEAD`, and `POST` methods, case-insensitive
header names, and no transfer compression, chunked framing, HTTP/2, or TLS in
the alpha core. A request has at most 8 KiB request-line bytes, 64 headers,
16 KiB aggregate header bytes, 8 MiB body bytes, and 16 MiB total bytes. The
server closes the connection after malformed or rejected input; keep-alive is
allowed only for successfully parsed requests. Read/write operations use a
30-second idle timeout and shutdown stops accepting new connections before
draining active ones. Parse failures map to 400, unsupported methods to 405,
oversized input to 413, and handler failures to 500; each response carries a
stable machine-readable error code and a server-side diagnostic event without
including request bodies or credentials.
**Checklist:**

- [x] Define HTTP version, methods, request-target forms, headers, body limits,
      status codes, keep-alive, shutdown, timeout, and TLS scope.
- [x] Define malformed-input, unsupported-method, and oversized-input behavior.
- [x] Define handler error mapping and observability requirements.
      **Acceptance:** Client, parser, runtime, and library work against one contract.
      **Verification:** Review contract fixtures for normal, malformed, and hostile
      requests.
      **Dependencies:** C1–C3.

## H2. HTTP request parser

**Objective:** Parse bounded HTTP requests safely.
**Implementation status:** The transport-independent parser is implemented in
`runtime/aura_rt.c` with an owning `AuraHttpRequest` result and explicit
`OK`/`INCOMPLETE`/400/405/413/error statuses. It parses one request from a
caller-provided byte buffer, reports the consumed boundary for a later
keep-alive loop, copies all request fields and body, and releases them through
`aura_http_request_destroy`. The focused native fixture
`runtime/tests/http_parser.c` proves valid GET/POST requests, case-insensitive
header lookup, equal and conflicting `Content-Length`, rejected transfer
encodings, malformed/truncated input, all bounded limits, and repeated cleanup.
This is parser-only evidence: socket reads, async suspension, server lifecycle,
fuzzing, and slow-client behavior remain open.
**Checklist:**

- [x] Parse request line, method, target, version, headers, content length, and
      body according to the frozen subset.
- [x] Enforce header, line, body, and total-request limits.
- [x] Reject malformed syntax, invalid lengths, unsupported encodings, and
      ambiguous framing deterministically.
      **Acceptance:** Parser never reads beyond limits or accepts conflicting framing.
      **Verification:** Run golden, negative, fuzz, truncated, oversized, and slow
      input cases.
      **Dependencies:** H1, IO3.

## H3. HTTP response builder

**Objective:** Serialize correct and bounded responses.
**Implementation status:** `runtime/aura_rt.c` now provides a transport-independent
`AuraHttpResponse` builder. It owns copied headers and body bytes, validates final
HTTP/1.1 status codes, token/header syntax, duplicate names, reserved framing
headers, response body limits, and no-body statuses. Serialization is deterministic:
caller headers retain insertion order, followed by generated `Content-Length` and
`Connection` headers. The default connection policy is `close`; callers may opt
into `keep-alive` explicitly. `aura_http_response_set_error` emits a bounded,
stable JSON error code for the contract's 400/405/413/500 responses and forces
connection close. `runtime/tests/http_response.c` covers text/binary/empty/error
responses, exact serialization goldens, repeated serialization, invalid headers,
status/body combinations, size limits, and caller-buffer sizing.
**Checklist:**

- [x] Support status, headers, content length, body, and connection semantics.
- [x] Define automatic versus explicit headers and invalid combinations.
- [x] Support empty, text, binary, and error responses.
      **Acceptance:** Serialized responses are deterministic and parseable by standard
      clients.
      **Verification:** Run `runtime/tests/http_response.c` with strict C warnings;
      exact serialization goldens and repeated-serialization checks pass.
      **Dependencies:** H1.

## H4. Connection lifecycle

**Objective:** Serve one or more requests safely over a TCP connection.
**Checklist:**

- [ ] Implement accept, request/response loop, keep-alive, and close behavior.
- [ ] Add read/write timeouts and client-disconnect handling.
- [ ] Define connection limits and graceful shutdown.
      **Acceptance:** Single-request and persistent connections terminate predictably.
      **Verification:** Run one-request, multi-request, timeout, disconnect, and
      shutdown cases.
      **Dependencies:** H2, H3, IO3, IO4.

## H5. Async HTTP integration

**Objective:** Run parsing, handlers, and writes through the async task model.
**Checklist:**

- [ ] Suspend on partial reads and writes without blocking other tasks.
- [ ] Propagate cancellation, parse failure, handler failure, and peer close.
- [ ] Preserve request/response buffers and ownership across awaits.
      **Acceptance:** Concurrent connections remain responsive under pending I/O.
      **Verification:** Run delayed-I/O, cancellation, GC, failure, and concurrency
      fixtures under sanitizers.
      **Dependencies:** H4, S1–S6, IO4–IO5.

## H6. Handler and routing API

**Objective:** Expose a minimal typed application interface.
**Checklist:**

- [ ] Define handler input/output types and lifecycle ownership.
- [ ] Implement method/path dispatch and documented parameter behavior.
- [ ] Return correct 404, 405, and handler-failure responses.
- [ ] Define whether handlers may spawn tasks or perform async I/O.
      **Acceptance:** A user can define a route without depending on internal socket or
      parser details.
      **Verification:** Run static routes, parameters if contracted, method mismatch,
      not-found, and handler-error cases.
      **Dependencies:** H2–H5, M1–M3.

## H7. Runnable server example

**Objective:** Prove the complete user journey.
**Checklist:**

- [ ] Add an example that binds localhost and serves a health endpoint.
- [ ] Print the bound address and provide deterministic shutdown.
- [ ] Exercise it through the CLI and a native HTTP client.
- [ ] Document build, run, test, and target selection commands.
      **Acceptance:** A clean installation can start, query, and stop the server on
      Linux and macOS.
      **Verification:** Execute the example from source and installed release.
      **Dependencies:** H6, P6–P8.

## H8. HTTP acceptance and hardening

**Objective:** Prevent common parser, resource, and lifecycle failures.
**Checklist:**

- [ ] Add parser fuzz cases and bounded-resource tests.
- [ ] Test slow clients, oversized requests, malformed framing, and concurrent
      clients.
- [ ] Run sanitizer and forced-shutdown tests.
- [ ] Record limits, known exclusions, and native-host results.
      **Acceptance:** The server cannot hang indefinitely or exceed configured limits
      under the mandatory hostile-input suite.
      **Verification:** Run the HTTP stage in the release acceptance matrix.
      **Dependencies:** H1–H7, A8, P8.
