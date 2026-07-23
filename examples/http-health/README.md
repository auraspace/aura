# HTTP health example

This bounded native companion binds a loopback TCP listener, prints the
ephemeral address, serves `GET /health` through the task executor, verifies a
native client receives `HTTP/1.1 200 OK` and `ok`, rejects a malformed request
with `400`, then performs deterministic shutdown.

Build and run it directly from the repository:

```sh
scripts/http-health-smoke.sh
```

The script uses the host C compiler with AddressSanitizer and UBSan. The
example exercises the documented native runtime APIs and bounded async
connection bridge; it is not yet an Aura CLI/async-handler example. The current
acceptance result is Linux x86_64. macOS remains unverified in this checkout.
