# aura.web example package

This is the standalone `aura.web` dependency package. Applications can depend
on it directly; `examples/todo-app` is the runnable consumer example.

## Application usage

```aura
val app = webApp(32)

// Ordered global middleware and explicit short-circuit middleware.
app.use(trace, requestMarker)
app.useNext(auth)

// Route middleware runs before the controller.
app.get("/users/:id", userRouteMarker, user)

// Mount middleware and a group under a shared prefix.
app.use("/mounted", mounted)
app.group("/admin", adminMiddleware)
    .get("/health", health)
    .post("/echo", echo)

// Lifecycle hooks and centralized failures.
app.onRequest(requestHook)
app.preHandler(preHandler)
app.onSend(onSend)
app.onResponse(onResponse)
app.onError(handleError)

// Plugins can register routes; configured plugins receive a prefixed app.
app.use(ApiPlugin())
app.use(PrefixedPlugin(), PluginOptions("/v1"))
```

The source is intentionally split into `app/`, `middleware/`, `request/`, and
`routing/`. Import it from another package with a path dependency:

```toml
[dependencies]
aura.web = { path = "../aura-web" }
```

`aura.web` is a framework-style layer over `std.http`; applications can depend
on this package directly, as shown by `examples/todo-app`.
