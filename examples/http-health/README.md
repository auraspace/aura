# HTTP health example

This bounded native companion binds a loopback TCP listener, prints the
ephemeral address, serves `GET /health`, verifies a native client receives
`HTTP/1.1 200 OK` and `ok`, then performs deterministic shutdown.

Build and run it directly from the repository:

```sh
scripts/http-health-smoke.sh
```

The script uses the host C compiler with AddressSanitizer and UBSan. The
example exercises the documented native runtime APIs; it is not yet an Aura
CLI/async-handler example. The current acceptance result is Linux x86_64.
macOS remains unverified in this checkout.
