# Aura async HTTP health server

Build and run from the repository root:

```sh
cargo run -p aura-cli -- build examples/http-health-aura -o /tmp/http-health-aura
/tmp/http-health-aura 8080
curl -i http://127.0.0.1:8080/health
curl -i http://127.0.0.1:8080/missing
curl -i -X POST http://127.0.0.1:8080/health
curl -i -X POST -H 'Transfer-Encoding: chunked' --data-binary 'hello' http://127.0.0.1:8080/echo
curl -i -X POST --data-binary 'hello' http://127.0.0.1:8080/stream
```

The optional first command-line argument selects the port (default `8080`). The handler waits twice on `std.time.sleep(1)` before writing its response. Each accepted stream is
served by a spawned `std.http.serveConnection` task, so slow clients do not
block later accepts. The bounded HTTP/1.1 limits and POSIX-only target support
are documented in RFC-007.

For an in-process graceful shutdown, call `closeListener(listener)`. The
server task treats the closed listener as a normal terminal outcome and cleans
up active connection tasks before executor shutdown.

`GET /health` returns `200`, an unknown target returns `404`, and an unsupported
method returns `405`. `POST /echo` returns its bounded decoded body, including
an inbound chunked body. `POST /stream` consumes one bounded Content-Length
chunk through `RequestBody` and returns it; chunked readers remain snapshot-only.
