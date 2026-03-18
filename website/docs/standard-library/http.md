---
title: HTTP
sidebar_position: 6
---

# HTTP Module

The `std/http` module provides high-level HTTP client and server abstractions.

## Classes

### `HTTPClient`

- `static get(host: string, port: number, path: string): HTTPResponse`
- `static request(host: string, port: number, method: string, path: string, body: string): HTTPResponse`

### `HTTPServer`

- `constructor(port: number)`
- `listen(handler: function(HTTPRequest): HTTPResponse): void`

### `HTTPRequest` & `HTTPResponse`

Data classes for representing HTTP requests and responses.

### Example: HTTP Client

```aura
import { HTTPClient } from "std/http.aura";

let res = HTTPClient.get("example.com", 80, "/");
print("Status: " + res.statusCode);
print("Body: " + res.body);
```

### Example: HTTP Server

```aura
import { HTTPServer, HTTPResponse } from "std/http.aura";

let server = new HTTPServer(8080);
server.listen(fn(req) {
  return new HTTPResponse(200, "OK", [], "Hello from Aura Server!");
});
```
