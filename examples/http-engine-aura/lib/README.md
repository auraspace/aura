# aura.web

`aura.web` is a small framework-style layer over `std.http` for dogfooding
Aura's async, class, array, closure, and path-dependency features.

- `App`: server lifecycle, middleware registration, verbs, and route groups
- `Router`: ordered route dispatch with 404/405 handling
- `Context`: params, query values, bounded body helpers (`readBody`/`readJson`), and chainable replies (`send`/`json`)
- `webApp()` defaults to a 1 MiB body limit; use `webAppWithBodyLimit(...)` to override it.
- `Middleware` / `Route`: async first-class handlers stored in class fields
- `Group`: Express-style prefix mounting for related routes

The package is intentionally kept as several source files so compiler failures
identify the affected language feature instead of hiding everything in `main`.
