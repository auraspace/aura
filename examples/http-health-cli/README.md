# Aura CLI HTTP health entrypoint

Run the bounded native HTTP health smoke through Aura's CLI:

```sh
aura run examples/http-health-cli
```

The program calls the documented primitive FFI bridge
`aura_http_health_smoke(): Int`; the bridge owns the opaque native HTTP
handles, runs the async one-request loop, and returns the process status. This
keeps the alpha CLI example honest while the full typed Aura TCP/HTTP handle
API remains future work. Linux is covered by the local acceptance matrix;
macOS remains unverified.
