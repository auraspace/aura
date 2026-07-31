# std.net (bounded alpha transport)

`std.net` provides owned endpoint-aware TCP listener and stream handles for the
supported POSIX targets. `listen`, `connect`, `accept`, `readStream`,
`readAllStream`, and `writeStream` use the runtime's nonblocking readiness
scheduler; `closeListener` and `closeStream` are idempotent terminal operations.

`readStream` is the streaming primitive: it returns one readiness-driven chunk
and uses an empty String only for EOF. `readAllStream` repeatedly calls it until
EOF, but caps its aggregate allocation at the caller-supplied capacity. It is a
convenience for bounded protocols, not an unbounded body API.

The implementation keeps native descriptors inside `ForeignHandle<Int>`
resources. Read and write tasks pin the resource across suspension, preserve
partial I/O offsets, and release the pin on completion, failure, cancellation,
or executor shutdown. Port-only endpoints default to loopback; explicit
`HOST:PORT` and `[IPv6]:PORT` endpoints can bind or connect elsewhere.
does not yet include DNS, TLS, UDP, Unix-domain sockets, or typed `Result`
errors.

`native/aura_net_ffi.c` remains a focused legacy FFI smoke fixture; it is not
linked automatically by the Aura CLI or used by the public runtime-backed API.
