# Aura HTTP engine example

This example builds a small Express/Fastify-style layer on top of `std.http`.
The `lib/` package owns context, middleware, route registration, and dispatch;
`src/main.aura` only registers hooks/handlers and starts the server.

Build and run from the repository root:

```sh
cargo run -p aura-cli -- build examples/http-engine-aura -o /tmp/http-engine-aura
/tmp/http-engine-aura
curl -i http://127.0.0.1:8080/health
curl -i http://127.0.0.1:8080/api/health
curl -i -X POST --data-binary 'hello aura' http://127.0.0.1:8080/echo
curl -i http://127.0.0.1:8080/users/42
curl -i -X PUT http://127.0.0.1:8080/users/42
curl -i 'http://127.0.0.1:8080/search?q=aura'
curl -i http://127.0.0.1:8080/missing
curl -i -X POST http://127.0.0.1:8080/health
```

Every response passes through the sample middleware and includes
`X-Aura-Engine: on`.

The example uses the current OOP surface directly: the library is split into
responsibility-based source folders, `RouteMatcher` is an interface with a
default method, `Router` uses a secondary constructor, and `webApp` accepts an
optional body-limit argument.

Pass a different port as the first argument when `8080` is busy.

Run the complete repeatable smoke test with:

```sh
scripts/http-engine-aura-smoke.sh
```
