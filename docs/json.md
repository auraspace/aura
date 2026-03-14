# JSON Handling Design Concept (Aura Stdlib)

This document proposes a **design concept** for first-class JSON parsing and serialization in Aura’s standard library. It is written to match Aura’s current design constraints:

- **Strict typing** (no `any` / `unknown`)
- **Ergonomic developer experience** (TypeScript-like feel)
- **Predictable runtime behavior** (stable stringify, clear error model)
- **Compiler-friendly** (future codegen via build-time attributes)

> Status: **Concept / Proposal**. This describes intended semantics and APIs, not necessarily what is implemented today.

---

## Goals

- Provide a small, fast `std/json` module with:
  - Parsing from `string` → JSON value model
  - Serialization from JSON value model → `string`
  - A path to **typed decoding/encoding** without a top type (`any`)
- Make failure modes explicit via **throwing typed errors** (e.g. `JsonError extends Error`) and idiomatic `try/catch`.
- Support safe defaults suitable for untrusted input (depth/size limits, clear numeric rules).

## Non-goals

- Not a replacement for a schema language (JSON Schema can be layered on later).
- Not a dynamic “duck-typed object” facility; Aura stays strictly typed.
- Not a streaming JSON parser in v1 (can be added later for huge payloads).

---

## Module shape

### Import path

Canonical module path should be:

```typescript
import { parse, stringify } from "std/json.aura";
```

Optionally, tooling may allow a short alias import like `"json"`, but the stdlib-backed path is the stable reference.

---

## JSON value model

Aura needs a runtime representation of JSON that is:

- **Total**: can represent any valid JSON.
- **Ergonomic**: easy to inspect in `match` / `if` flows.
- **Safe**: avoids implicit conversions.

### Proposed type: `JsonValue`

Conceptually, JSON values are a tagged union:

- `null`
- `bool`
- `number` (JSON number; see numeric rules below)
- `string`
- `array` of JSON values
- `object` mapping string keys → JSON values

In Aura syntax, this can be expressed as an enum-like model (exact mechanism depends on Aura’s final ADT design). For documentation purposes, we’ll refer to it as:

```typescript
type JsonValue =
  | JsonNull
  | JsonBool
  | JsonNumber
  | JsonString
  | JsonArray
  | JsonObject;
```

### Object representation

JSON object keys are always strings. The object container should use a map type:

- `Map<string, JsonValue>` (preferred; preserves explicitness)

Ordering:

- Parsing preserves **insertion order** if the underlying `Map` preserves it.
- `stringify` defaults to **deterministic key ordering** (see below) for stable builds and tests.

Duplicate keys:

- Default behavior: **error** (reject input) to prevent ambiguity/security issues.
- Optional behavior: `last_wins` mode (explicit opt-in via parse options).

---

## Parsing API

### Primary API

```typescript
function parse(input: string): JsonValue; // throws JsonError
```

### Parse options (future-compatible)

```typescript
class JsonParseOptions {
  maxDepth: i32; // default: 128
  maxInputBytes: i32; // default: 8 * 1024 * 1024 (8 MiB)
  allowTrailingCommas: bool; // default: false
  allowComments: bool; // default: false
  duplicateKeys: "error" | "last_wins"; // default: "error"
}

function parseWith(input: string, opts: JsonParseOptions): JsonValue; // throws JsonError
```

Rationale:

- Limits prevent memory/CPU abuse from adversarial JSON.
- Comments and trailing commas are common in config files, but should be explicit opt-ins.

### Error model

Errors should be descriptive and actionable.

`std/json` should throw a dedicated error type:

```typescript
class JsonError extends Error {
  message: string;
  line: i32;
  column: i32;
  offset: i32; // byte offset in UTF-8 input
  // kind: JsonErrorKind (optional)
  // context: string (optional)
}
```

Minimum fields:

- `message: string`
- `line: i32`
- `column: i32`
- `offset: i32` (byte offset in UTF-8 input)

Optional fields:

- `kind: JsonErrorKind` (unexpected token, depth exceeded, invalid escape, etc.)
- `context: string` (short snippet window)

---

## Serialization API

### Primary API

```typescript
function stringify(value: JsonValue): string;
```

### Stringify options

```typescript
class JsonStringifyOptions {
  pretty: bool; // default: false
  indent: string; // default: "  " (two spaces)
  sortKeys: bool; // default: true (deterministic output)
  escapeUnicode: bool; // default: false (keep UTF-8 by default)
}

function stringifyWith(value: JsonValue, opts: JsonStringifyOptions): string;
```

Determinism:

- With `sortKeys: true`, objects must serialize keys in lexicographic order (byte-wise UTF-8 or Unicode scalar order; pick one and document it).
- This makes snapshots/tests stable and reduces diffs in generated artifacts.

---

## Numeric rules (JSON number → Aura types)

JSON has a single numeric grammar; Aura has multiple numeric types. The JSON value model should store numbers as a **loss-minimizing canonical** form, then provide explicit conversions.

### Proposed canonical storage

- `JsonNumber` stores a `f64` (IEEE-754 double).

Rationale:

- Matches de-facto JSON ecosystems.
- Good compatibility with existing Rust implementation strategies.

### Explicit conversions

Provide conversion helpers that fail on overflow/NaN/infinity:

```typescript
function asI32(v: JsonValue): i32; // throws JsonError
function asI64(v: JsonValue): i64; // throws JsonError
function asF64(v: JsonValue): f64; // throws JsonError
```

Rules:

- Parsing rejects `NaN`, `Infinity`, `-Infinity` (not valid JSON).
- `asI32` / `asI64` require:
  - value is integral (no fractional part)
  - value is within target bounds

---

## Safe access & navigation

Because Aura is strict, JSON navigation should never silently return a wrong type. Provide composable APIs:

### Indexing helpers

```typescript
function get(obj: JsonValue, key: string): JsonValue; // throws JsonError
function at(arr: JsonValue, index: i32): JsonValue; // throws JsonError
```

### Optional access

For common “field may be absent” use cases, offer `Option`-like patterns if Aura has them, or a `null`-aware API:

```typescript
function tryGet(obj: JsonValue, key: string): JsonValue | null;
```

> Note: Aura’s `null` is already part of the type system. APIs should reflect that explicitly instead of inventing “undefined”.

---

## Typed decoding / encoding (no `any`)

To make JSON useful for real applications, Aura needs a path from `JsonValue` to user-defined types and back.

### Layer 1: Manual decoding

Provide a small set of primitives:

- `expectObject(value)`, `expectArray(value)`, `expectString(value)`, etc.
- Conversion helpers (`asI32`, `asBool`, …)
- Field accessors with good error paths (`missing_field`, `wrong_type`, `path`)

Example concept:

```typescript
class User {
  id: string;
  age: i32;
  constructor(id: string, age: i32) {
    this.id = id;
    this.age = age;
  }
}

function decodeUser(v: JsonValue): User {
  // These helpers throw JsonError with an attached path (recommended).
  let obj = expectObject(v);
  let id = expectString(get(obj, "id"));
  let age = asI32(get(obj, "age"));
  return new User(id, age); // may throw if fields are invalid
}
```

### Layer 2: Codegen via build-time attributes (recommended)

Aura already plans build-time decorators/attributes (see `docs/syntax.md`). JSON can leverage that:

- `@JsonSerializable` generates:
  - `toJson(): JsonValue`
  - `static fromJson(JsonValue): T` (throws `JsonError`)

Key points:

- Generated code is fully typed.
- No reflection needed at runtime.
- Supports structural typing and fast paths.

Concept:

```typescript
@JsonSerializable
class User {
  id: string;
  age: i32;
}
```

Config knobs (future):

- field renames (`@JsonName("user_id")`)
- optional fields (`string?`)
- default values
- enum representation (string vs number)

---

## Interop with `std/http` (future)

Once `std/json` exists, `std/http` can provide sugar:

- `HTTPRequest.json(): JsonValue` (throws `JsonError`)
- `HTTPResponse.json(): JsonValue` (throws `JsonError`)
- `HTTPResponse.json<T>(): T` (throws `JsonError`, with codegen-based `fromJson`)

This keeps `std/http` lightweight while making JSON-first APIs pleasant.

---

## Security and performance considerations

- **Depth/size limits**: enabled by default.
- **Duplicate keys**: default reject.
- **UTF-8 correctness**: input strings are UTF-8; ensure escape sequences are validated.
- **Deterministic stringify**: default sort keys for stable output.
- **Allocation strategy** (implementation detail):
  - avoid excessive intermediate strings
  - reuse buffers during stringify

---

## Rust implementation note (for the toolchain)

The Aura toolchain already depends on `serde` / `serde_json` in `Cargo.toml`. A practical initial implementation can:

- Parse via `serde_json` into `serde_json::Value`
- Convert into Aura’s runtime `JsonValue` representation (or embed an equivalent)
- Implement `stringify` via `serde_json` serializer or a custom formatter for determinism

This keeps the first iteration fast while the Aura runtime/GC/value model evolves.

---

## Compatibility summary

- Strictly valid JSON by default (RFC 8259 style).
- Optional relaxed parsing for config scenarios (comments/trailing commas).
- Explicit conversions to Aura numeric types.
- Designed to compose with Aura’s future attribute-driven codegen.
