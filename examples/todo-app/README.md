# Aura Todo API

`todo-app` is a small in-memory REST API built on the standalone
`examples/aura-web` package. It demonstrates using the framework as a
dependency instead of keeping a private copy of its implementation.

Build and run from the repository root:

```sh
cargo run -p aura-cli -- build examples/todo-app -o /tmp/todo-app
/tmp/todo-app 8080
```

The API exposes:

```text
GET    /health
GET    /api/todos
GET    /api/todos?completed=true|false
POST   /api/todos             {"title":"Ship the example"}
PATCH  /api/todos/:id         {"completed":true}
DELETE /api/todos/:id
GET    /api/stats
GET    /v1/todos              prefix-group example
```

Try it with curl:

```sh
curl -i http://127.0.0.1:8080/health
curl -i -X POST http://127.0.0.1:8080/api/todos \
  -H 'content-type: application/json' \
  -d '{"title":"Learn Aura web"}'
curl -i http://127.0.0.1:8080/api/todos
curl -i -X PATCH http://127.0.0.1:8080/api/todos/1 \
  -H 'content-type: application/json' \
  -d '{"completed":true}'
curl -i http://127.0.0.1:8080/api/stats
curl -i -X DELETE http://127.0.0.1:8080/api/todos/1
```

The application also demonstrates global middleware, explicit `Next` control,
centralized errors, path mounting, and a prefixed route group supplied by
`aura.web`.

The source is split by responsibility:

```text
src/
  main.aura              process entrypoint
  app/app.aura           application wiring and TodoApiPlugin
  todos/model.aura       enum, interface, struct, domain and JSON models
  todos/store.aura       InMemoryTodoRepository implementation
  todos/service.aura     TodoService and generic TodoResult<T>
  todos/handlers.aura    HTTP handlers and JSON boundary
```

Request bodies use `decode<T>(Value)` from `std.json`, and responses use
`encode<T>(value)` before they are written to the response. This keeps JSON
validation and serialization typed instead of assembling JSON strings by hand.

The domain intentionally exercises Aura's object model: `TodoReader` is a
parent interface of `TodoRepository`, `InMemoryTodoRepository` implements it,
`TodoFilter` and `TodoStats` are value structs, `TodoService` composes the
repository abstraction, `TodoApiPlugin` implements `aura.web.Plugin`, and
`TodoStatus`, `TodoError`, and generic `TodoResult<T>` model state and typed
failures.
