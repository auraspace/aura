# std.udp

`std.udp` provides bounded endpoint-keyed datagrams on POSIX targets. `bind`
registers a nonblocking socket, `send` copies and transmits one String payload,
and `receive` waits for one datagram up to the requested capacity. `close` is
idempotent; unsupported targets fail through the normal runtime boundary.

The locked `Socket` shape stores its endpoint, so the runtime owns a bounded
table of active sockets keyed by host and port. Applications should use
distinct local endpoints and close them when finished.
