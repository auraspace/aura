# std.net (bounded alpha transport)

`std.net` provides owned endpoint-aware TCP listener and stream handles for the
supported POSIX targets. `listen`, `connect`, `accept`, `readStream`,
`readAllStream`, and `writeStream` use the runtime's nonblocking readiness
scheduler; `closeListener` and `closeStream` are idempotent terminal operations.

`readStream` is the String compatibility primitive: it returns one
readiness-driven chunk and uses an empty String only for EOF.
`readAllStream` repeatedly calls it until EOF, but caps its aggregate
allocation at the caller-supplied capacity. It is a convenience for bounded
protocols, not an unbounded body API.

Binary protocols should use `readExactly` / `readExactlyWithTimeout` and
`writeAll` / `writeAllWithTimeout` with `std.bytes.Buffer`. Exact reads preserve
the distinction between EOF before any byte, partial EOF, timeout, and closed
stream; writes continue across short writes until the buffer is complete or the
operation fails. Timeout arguments are operation deadlines in milliseconds,
not per-syscall retry intervals.

The implementation keeps native descriptors inside `ForeignHandle<Int>`
resources. Read and write tasks pin the resource across suspension, preserve
partial I/O offsets, and release the pin on completion, failure, cancellation,
or executor shutdown. Cancellation closes the pending stream and wakes its
readiness wait so no socket remains blocked behind a cancelled task. Port-only
endpoints default to loopback; explicit `HOST:PORT` and `[IPv6]:PORT` endpoints
can bind or connect elsewhere. The additive `listenResult`, `connectResult`,
`closeListenerResult`, and `closeStreamResult` APIs return shared
`std.error.Outcome` values with `NetError`; the older throwing/Bool forms remain
compatibility shims. TLS is provided by `std.tls`; UDP and Unix-domain sockets
are outside this bounded surface.

`native/aura_net_ffi.c` remains a focused legacy FFI smoke fixture; it is not
linked automatically by the Aura CLI or used by the public runtime-backed API.
