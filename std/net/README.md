# std.net (bounded alpha bridge)

`std.net` exposes a small primitive-only FFI surface: loopback echo,
HTTP/1.1 request status parsing, and response serialization. It is deliberately
limited to `String` and `Int`, so an Aura program can call it under the current
FFI v1 contract without retaining a foreign pointer or crossing `await`.

The native symbols are supplied by `native/aura_net_ffi.c` in this repository
for compile/native acceptance tests and by an application-owned library in a
real deployment. The library is not linked automatically by the Aura CLI.

Known blocker (G5/G7): `AuraTcpListener`, `AuraTcpStream`, and HTTP connection
handles in `runtime/aura_ffi.h` cannot yet be represented by Aura FFI v1,
because sema permits only `Int`, `Bool`, `String`, and `Unit` and rejects
references/out-parameters. Exposing typed async handles needs a sema/codegen
contract change plus task/await lifetime tests; this package does not hide that
limitation behind an unsafe integer cast.
