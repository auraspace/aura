# std.websocket

`std.websocket` provides a bounded POSIX client transport over `ws://` TCP
endpoints. The runtime performs the HTTP/1.1 upgrade, masks client frames,
accepts text, binary, ping, pong, and close opcodes, and caps each payload at
65535 bytes. Connections are keyed by their locked endpoint field and close
idempotently.

TLS (`wss://`) is intentionally not accepted until `std.tls` has a real
provider and certificate policy.
