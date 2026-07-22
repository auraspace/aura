# RFC-014: Language Server

| Field             | Value                                                |
| ----------------- | ---------------------------------------------------- |
| **RFC**           | 014                                                  |
| **Title**         | Language Server                                      |
| **Status**        | Draft                                                |
| **Layer**         | Toolchain                                            |
| **Authors**       | Aura contributors                                    |
| **Created**       | 2026-07-22                                           |
| **Updated**       | 2026-07-22                                           |
| **Estimate**      | 50–80 pages                                          |
| **Depends**       | RFC-001, RFC-002, RFC-004, RFC-005, RFC-008, RFC-012 |
| **Blocks**        | —                                                    |
| **Supersedes**    | —                                                    |
| **Superseded by** | —                                                    |

---

## 1. Abstract

This RFC defines Aura’s **language server**: a long-lived, editor-neutral process that exposes compiler knowledge through the Language Server Protocol (LSP). It gives editors and IDEs fast diagnostics, completion, hover information, go-to-definition, find references, rename, formatting, symbols, and code actions while reusing the same parser, resolver, type checker, and diagnostic model as `aura check`.

The server is a companion to the Aura toolchain, not a second compiler. It communicates over JSON-RPC, maintains a workspace snapshot, and may answer against a partially edited or temporarily invalid program. The initial contract prioritizes correctness, cancellation, deterministic results, and graceful degradation over a large feature surface.

## 2. Motivation

### 2.1 Problem statement

Without a first-party language server, Aura users must leave their editor to run `aura check`, search files manually, and infer API shapes from source. A separately implemented editor engine would quickly diverge from the compiler and produce diagnostics and symbol behavior that do not match the CLI.

### 2.2 Why now

The compiler already has source spans, structured diagnostics, package resolution, and formatter/test-report tooling. RFC-014 turns those foundations into an interactive developer workflow before editor integrations fragment across vendors.

### 2.3 Success metrics

| Metric           | Target                                                                                                      |
| ---------------- | ----------------------------------------------------------------------------------------------------------- |
| Correctness      | The server and `aura check` agree on diagnostics for the same workspace snapshot                            |
| Responsiveness   | Completion and hover return within 150 ms for a warm, small workspace; large workspaces degrade predictably |
| Resilience       | Keystrokes never require a successful parse or type check to keep the server alive                          |
| Interoperability | Works with any conforming LSP client over stdio                                                             |
| Stability        | Request/notification behavior and Aura-specific extensions are versioned                                    |

## 3. Goals

- Provide a first-party LSP server launched by the Aura CLI.
- Reuse compiler frontend and structured diagnostics rather than duplicating language semantics.
- Support in-memory document versions, unsaved edits, multiple files, packages, and workspace roots.
- Make diagnostics, cancellation, partial results, and failures safe for interactive clients.
- Establish a small baseline: diagnostics, completion, hover, definition, references, rename, formatting, document symbols, and workspace symbols.
- Keep editor features available even when later compiler phases fail.
- Define an extension path for Aura-specific commands and semantic data without making clients depend on it.
- Specify compiler integration, document sync, indexing, scheduling, caching, and feature degradation so implementations share one contract.

## 4. Non-goals

- Defining a new editor protocol; LSP and JSON-RPC remain the wire contract.
- Replacing `aura build`, `aura test`, or the compiler’s authoritative batch diagnostics.
- Implementing a full IDE, debugger, profiler, refactoring suite, or project generator.
- Providing a general-purpose analysis API for third-party compiler plugins.
- Promising semantic features that the language/compiler cannot support reliably yet, such as live borrow visualization or async scheduling inspection.
- Making the language server a required runtime dependency of compiled Aura applications.

## 5. Prior art & alternatives

| Approach                 | Pros                                                           | Cons                                                                  | Decision                  |
| ------------------------ | -------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------- |
| Standard LSP server      | Broad editor support; established request and capability model | Lowest common denominator; some features need extensions              | **Chosen**                |
| Editor-specific plugins  | Native editor APIs and richer UI                               | N integrations; duplicated parsing and semantics                      | Not the core architecture |
| CLI-on-save only         | Very small implementation                                      | Slow feedback; weak completion/navigation; poor unsaved-state support | Insufficient              |
| Separate analysis engine | Can optimize specifically for IDE workloads                    | Semantic drift from compiler; duplicate maintenance                   | Rejected                  |

## 6. Design

### 6.1 Overview

The server is a process boundary around a shared analysis service:

```text
LSP client (editor)
        │ JSON-RPC over stdio
        ▼
┌──────────────────────────────┐
│ aura language-server          │
│  protocol adapter             │
│  workspace/document snapshots │
│  request scheduler + cancel   │
└──────────────┬───────────────┘
               ▼
     shared compiler frontend
     lexer → parser → resolve → sema
               │
               ├── structured diagnostics
               ├── symbol/index queries
               └── source edits / formatting
```

The CLI entrypoint is `aura language-server` (an alias `aura lsp` may be provided). The default transport is stdio so editors can own process lifetime and no network socket is opened implicitly.

### 6.2 Lifecycle and workspace model

1. The client sends `initialize` with root folders and client capabilities.
2. The server advertises only capabilities implemented by the current build.
3. `didOpen`, `didChange`, and `didClose` update an in-memory document store (see §6.4).
4. Workspace configuration is loaded from `aura.toml` and lockfiles using the same resolution rules as RFC-005 and RFC-008. Unknown or unavailable dependencies produce actionable diagnostics, not a server crash.
5. `shutdown` ends analysis; `exit` terminates the process according to LSP.

The open document is authoritative over its on-disk counterpart. Files not opened by the client are read from disk through a workspace file abstraction. URI/path conversion, encoding, and file identity must be deterministic across platforms (see §15).

### 6.3 Compiler integration

The language server is not a second frontend. It hosts the same compiler crates used by `aura check` and exposes them through a narrow **Analysis API**.

#### 6.3.1 Analysis API

The Analysis API is the only surface the protocol adapter may call. It provides:

| Query family        | Purpose                                                            |
| ------------------- | ------------------------------------------------------------------ |
| Parse / syntax tree | AST (or recovered AST) for a document version                      |
| Resolve             | Name bindings, imports, visibility, package membership             |
| Type / sema         | Types, signatures, member sets, type diagnostics                   |
| Diagnostics         | Lexical, parse, resolve, and type diagnostics for a package or URI |
| Symbols             | Document and workspace symbol enumeration                          |
| Navigation          | Definition, type definition, implementation (when available)       |
| References / rename | Reference collection and workspace edit planning                   |
| Format              | Formatter contract from RFC-012                                    |
| Index               | Symbol index queries and invalidation hooks                        |

Every query takes an explicit **snapshot id** (or snapshot handle). Queries never read “the current mutable workspace” implicitly. Results are pure with respect to that snapshot plus toolchain version and configuration.

The Analysis API is internal to the Aura toolchain. This RFC does not define a stable public plugin ABI for third-party analysis (see Non-goals).

#### 6.3.2 Compiler session

A **Compiler Session** is the long-lived host state owned by the language server process:

- workspace roots and package graph;
- open and closed file contents (virtual overlay + disk);
- configuration and feature flags;
- caches, indices, and in-flight query tasks;
- cancellation registry and request metadata.

One process has one primary session for the negotiated workspace. Multi-root workspaces share a single session with multiple roots rather than one process per root, unless the client launches separate servers.

Sessions are created at `initialize` (or immediately after) and destroyed at `shutdown`/`exit`. Restarting the process is always a valid recovery path.

#### 6.3.3 Query interface

Queries are:

- **idempotent** for a fixed snapshot;
- **cancellable** via a token associated with the LSP request id;
- **phase-aware**: a query may succeed at parse depth even when type checking failed;
- **bounded**: implementations may reject or truncate pathological results (workspace symbols, references) with deterministic limits.

Batch CLI (`aura check`) and the language server should call the same query implementations for diagnostics and resolution. Differences in inputs (open buffers, partial packages) are expressed only through the snapshot, not through alternate type checkers.

#### 6.3.4 Snapshot ownership

Snapshots are **immutable** once published:

1. An edit or config change constructs a new snapshot (cheap structural sharing is allowed).
2. In-flight requests retain their snapshot until they complete or cancel.
3. No query may mutate shared analysis data in place.
4. The session may drop a snapshot only after no request still holds it and after cache eviction policy permits.

Observable rule: a response must never mix symbols or diagnostics from two different document versions of the same URI.

#### 6.3.5 Shared compiler components

| Component              | Shared with CLI? | Notes                                                |
| ---------------------- | ---------------- | ---------------------------------------------------- |
| Lexer / parser         | Yes              | Error-recovery paths must remain IDE-usable          |
| Name resolution        | Yes              | Same visibility and import rules as RFC-001/004      |
| Type checker / sema    | Yes              | Same types and diagnostic codes as RFC-002/004       |
| Diagnostics model      | Yes              | Converted to LSP only at the protocol boundary       |
| Package / lock resolve | Yes              | Same as RFC-005 / RFC-008                            |
| Formatter              | Yes              | RFC-012 contract                                     |
| Codegen / LLVM         | No (MVP)         | Not required for interactive analysis                |
| Test / build execution | No               | Server must not run build scripts for editor queries |

### 6.4 Document synchronization

#### 6.4.1 Full sync vs incremental sync

The server MUST support `TextDocumentSyncKind.Full` in MVP. Incremental sync (`TextDocumentSyncKind.Incremental`) is optional but recommended once range application is correct for UTF-16 LSP positions.

| Mode        | Client sends                                    | Server applies                                     |
| ----------- | ----------------------------------------------- | -------------------------------------------------- |
| Full        | Entire document text on each change             | Replace open buffer; bump version                  |
| Incremental | Ordered `TextDocumentContentChangeEvent` ranges | Apply edits in order on the current buffer version |

If both are negotiated, the server follows the client’s `textDocumentSync` capability response. Incremental application errors (overlapping ranges, out-of-bounds) are treated as recoverable: log, request full resync if the client supports it, or fall back to treating the document as dirty-unknown without crashing.

#### 6.4.2 Version handling

Each open document has a client-supplied integer **version**. Rules:

- Versions for a URI are expected to increase monotonically while the document is open.
- A `didChange` or request carrying a **stale** version (less than the server’s current version for that URI) is ignored for mutation; associated requests may be cancelled or answered against the latest snapshot with an explicit version mismatch log.
- A version equal to the current version with empty content changes is a no-op.
- After `didClose`, version state for that URI is discarded; a later `didOpen` starts a new sequence.

Responses that include document identity (diagnostics, edits) MUST be attributable to a known version. Diagnostics for version _N_ must not be presented as if they describe version _N+k_ after newer changes have been applied; the server either republishes for the new version or clears/replaces atomically.

#### 6.4.3 Out-of-order changes

JSON-RPC notifications may be processed only in the order required for document integrity:

1. `didOpen` before any `didChange` / `didSave` / `didClose` for that URI.
2. `didChange` events applied in notification order, not by racing worker threads.
3. `didClose` ends the open overlay; subsequent changes without open are errors and are ignored.

If the client sends changes before open, the server logs a protocol violation and does not invent buffer state. Request handlers that need a document wait for a consistent open buffer or return an empty/partial result.

#### 6.4.4 Unsaved buffers

Open documents form a **virtual overlay** over the filesystem:

- Overlay content is authoritative for analysis of that URI.
- Disk content is used for files not present in the overlay.
- Saving (`didSave`) does not by itself change analysis if content already matched; it may trigger non-analysis side effects only if explicitly configured (MVP: none required).
- Closing an unsaved buffer reverts that URI to disk content (or “missing”) for subsequent analysis.

Unsaved edits must never require a successful parse or type check to remain in the overlay.

### 6.5 Workspace and project model

#### 6.5.1 Multi-root workspace

The server accepts `workspaceFolders` from `initialize` / `didChangeWorkspaceFolders`. Each root is an independent Aura project or monorepo subtree with its own manifests when present.

Rules:

- Package graphs are built per root, then linked where path/registry dependencies cross roots.
- URI ownership: a file belongs to at most one primary package; ambiguous membership yields a diagnostic.
- Removing a folder invalidates its packages, indices, and open-document associations under that root.

#### 6.5.2 Package graph

The package graph mirrors RFC-005 / RFC-008:

- nodes: packages (workspace members and resolved dependencies);
- edges: direct dependencies with version/source provenance from the lockfile when available.

Editor analysis uses the **locked** dependency set when a lockfile exists. Missing lock entries or unresolved deps produce diagnostics; they do not invent versions.

#### 6.5.3 Dependency boundaries

| Boundary              | Default behavior                                      |
| --------------------- | ----------------------------------------------------- |
| Workspace packages    | Full analysis, rename, references                     |
| Path dependencies     | Read-only navigation and hover when sources available |
| Registry dependencies | Index public API when sources or metadata available   |
| Generated / build-out | Read-only; rename refused                             |

References and rename default to **workspace-writable** sources. Dependency traversal is opt-in, bounded, and never mutates dependency-owned files.

#### 6.5.4 Generated sources

Generated Aura sources (build outputs, codegen) are identified by one or more of:

- manifest / build metadata (preferred when available);
- conventional output directories excluded from normal edits;
- attributes or tooling markers (open question; see §7).

Generated files may be indexed for go-to-definition and hover when readable. Rename, format-on-type that writes back, and workspace edits targeting generated paths are rejected unless the client explicitly owns those URIs as ordinary documents.

#### 6.5.5 Virtual files

Virtual files include:

- unsaved editor buffers;
- in-memory snippets (if ever used for isolated evaluation; not required for MVP);
- client-provided untitled documents when the client assigns a URI scheme the server understands.

The file abstraction maps `file:` and negotiated schemes to snapshot content. Non-`file` schemes are supported only when the client supplies full text via document sync; the server does not fetch arbitrary remote URLs during analysis (see §10).

### 6.6 Analysis snapshots and scheduling

Every request observes one immutable workspace snapshot. A new edit can invalidate queued work, but it must not mutate data being read by an in-flight request. High-level scheduling rules:

- prioritize diagnostics for the most recently changed document;
- cancel obsolete requests when the client supplies a cancellation token;
- bound parallelism and memory usage;
- return an empty or partial result when a feature cannot be computed yet;
- never report results for an older document version as if they described the latest version.

Detailed request priorities, queue policy, and fairness are defined in §6.8. Caching is defined in §6.9.

The first implementation may rebuild the affected package or module. Incremental query caching is an optimization and must not change observable semantics. Cache keys include document content/version, compiler configuration, dependency lock state, and toolchain version.

### 6.7 Indexing model

#### 6.7.1 Symbol index

The symbol index supports workspace symbols, references bootstrap, and cross-file navigation. Entries include at least:

- symbol name and kind;
- defining URI and range;
- package id and visibility;
- container / parent path for hierarchical search;
- content fingerprint or snapshot generation for invalidation.

The index is a derived structure. Authoritative definitions still come from resolve/type queries on a snapshot.

#### 6.7.2 Build triggers

Indexing work is triggered by:

| Event                          | Index action                                     |
| ------------------------------ | ------------------------------------------------ |
| `initialize` / workspace load  | Eager index of workspace package roots (bounded) |
| `didOpen` / `didChange`        | Invalidate and reindex affected modules/packages |
| `didChangeWatchedFiles`        | Invalidate closed files that changed on disk     |
| Config / lockfile change       | Rebuild package graph; selective or full reindex |
| Dependency source availability | Extend or refresh dependency API index           |

Triggers schedule **background** work; they must not block the LSP message read loop.

#### 6.7.3 Invalidation

Invalidation is **fine-grained when possible**, coarse when unsure:

1. Content change → invalidate that file’s parse/resolve/type artifacts and dependent reverse edges.
2. Manifest / lock change → invalidate package graph and dependency resolution.
3. Toolchain or config change → global invalidate.
4. On uncertainty, drop larger subgraphs rather than serve mixed generations.

Stale index entries must not be returned as current definitions without revalidation against the request snapshot when correctness requires it (rename, references).

#### 6.7.4 Lazy vs eager indexing

| Mode  | When                                     | Trade-off                         |
| ----- | ---------------------------------------- | --------------------------------- |
| Eager | Workspace members at startup             | Faster first workspace-symbol hit |
| Lazy  | Dependencies, cold packages, large trees | Lower memory and startup cost     |

MVP: eager index of local workspace packages (with time/memory bounds); lazy for dependencies. Users may configure more aggressive laziness for huge monorepos.

#### 6.7.5 Cross-package index

Cross-package edges record import/use relationships and public symbol definitions. Reference search may walk these edges with a depth and result bound. Cross-package data always carries package and version provenance so results can exclude the wrong lock generation.

### 6.8 Request execution model

#### 6.8.1 Request priorities

| Priority | Examples                                               |
| -------- | ------------------------------------------------------ |
| P0       | Protocol lifecycle (`initialize`, `shutdown`), cancel  |
| P1       | Document sync notifications (serialize correctness)    |
| P2       | Interactive edits: completion, hover, signature help   |
| P3       | Navigation: definition, references, document symbols   |
| P4       | Workspace-wide: workspace symbols, project diagnostics |
| P5       | Background: full reindex, warm caches, prefetch        |

Higher priority work preempts lower priority **queued** work. Running P5 tasks are cancelled or paused when P2 work needs CPU, subject to cooperative cancellation.

#### 6.8.2 Queue policy

- One logical queue per priority band, FIFO within a band unless a request is obsolete.
- Document-local requests for the active URI may jump ahead of older document-local requests for the same feature when versions supersede them.
- Queue depth is bounded; excess low-priority work is dropped or coalesced (e.g. multiple full-project diagnostic runs collapse to one).

#### 6.8.3 Background analysis

Background analysis includes reindexing, warm type-checking of recently edited packages, and dependency metadata loads. It:

- uses idle CPU under resource limits (§6.17);
- never holds locks that block P1/P2 for unbounded time;
- publishes diagnostics only for still-current snapshots.

#### 6.8.4 Fair scheduling

Multi-root and multi-package workspaces must not starve one root because another is huge. Fairness rules:

- rotate background package work across roots;
- cap consecutive background quanta spent on a single package;
- always accept document sync and cancellation promptly.

#### 6.8.5 Starvation prevention

- Interactive requests (P2) have a soft latency SLO (§6.16); if background work overruns a quantum, it yields.
- A request waiting longer than the cancellation budget may be failed with cancel/partial rather than run forever.
- Diagnostics for the focused document are never permanently stuck behind full-workspace analysis.

### 6.9 Caching strategy

#### 6.9.1 Cache scope

| Scope    | Contents                                       | Shared across requests? |
| -------- | ---------------------------------------------- | ----------------------- |
| Process  | Toolchain version, capability flags            | Yes                     |
| Session  | Package graph, config, indices                 | Yes                     |
| Snapshot | Parse/resolve/type artifacts for that snapshot | Yes within snapshot     |
| Request  | Temporary buffers for one handler              | No                      |

#### 6.9.2 Cache invalidation

Caches key on at least: URI content hash or version, package id, config hash, lock fingerprint, toolchain version, and query name. Invalidation follows §6.7.3. Semantic version of the Analysis API is part of the key when queries evolve.

Observable semantics must not depend on whether a cache hit occurred.

#### 6.9.3 Snapshot lifetime

Snapshots live until:

- no in-flight request references them; and
- they are not the latest snapshot; and
- eviction policy selects them (or memory pressure forces drop).

The latest snapshot is pinned while the session is active, subject to memory limits (older derived artifacts may still be dropped and recomputed).

#### 6.9.4 Memory limits

Implementations SHOULD expose soft and hard memory budgets (configuration and/or defaults). Approaching the soft limit triggers index and cache eviction. Hitting the hard limit cancels background work and may clear non-essential caches; the server remains alive and continues to serve degraded results.

#### 6.9.5 Eviction policy

Default eviction order (first to drop):

1. dependency index entries not recently used;
2. type artifacts for packages not open or recently edited;
3. parse trees for closed cold files;
4. workspace symbol postings for cold packages.

LRU within a class. Never evict the open-document overlay texts themselves.

### 6.10 Diagnostics

Diagnostics use the shared `aura-diagnostics` model and are converted to LSP ranges only at the protocol boundary. The server publishes lexical, parse, name-resolution, and type diagnostics with severity, source, stable code, related information, and notes where the client supports them.

Diagnostics are published per URI and replaced atomically for the current open-document snapshot. A parse error should retain any symbols that can still be recovered, so completion and navigation remain useful. The server must distinguish an unavailable dependency, an invalid source edit, and an internal server failure in logs and client-visible messages.

### 6.11 Baseline language features

The MVP supports these LSP features when the relevant compiler query is available:

| Feature           | Behavior                                                                                                              |
| ----------------- | --------------------------------------------------------------------------------------------------------------------- |
| Completion        | Keywords, visible declarations, members after `.`, imports, and type names; include detail and `textEdit` when needed |
| Hover             | Symbol signature, resolved type, and concise documentation from source comments/docs                                  |
| Definition        | Navigate to declarations across the workspace and resolved package sources                                            |
| References        | Find references within the workspace; dependency traversal is opt-in and bounded                                      |
| Rename            | Produce a versioned `WorkspaceEdit`; reject ambiguous or externally-owned symbols                                     |
| Formatting        | Reuse RFC-012’s formatter contract; support whole document first                                                      |
| Document symbols  | Declarations in source order with accurate ranges and container names                                                 |
| Workspace symbols | Search indexed declarations with deterministic ordering and bounded results                                           |
| Code actions      | Safe, local fixes for compiler-provided diagnostic suggestions                                                        |

Unsupported features must be omitted from `ServerCapabilities`, not advertised and then rejected on every request.

### 6.12 Feature availability matrix

Features degrade by **compiler phase**, not by crashing. Clients should assume partial answers are valid.

| Feature                         | Parse | Resolve | Type | Notes                                             |
| ------------------------------- | ----- | ------- | ---- | ------------------------------------------------- |
| Diagnostics (syntax)            | ✓     | ✓       | ✓    | Always when text is available                     |
| Diagnostics (names)             | ✗     | ✓       | ✓    |                                                   |
| Diagnostics (types)             | ✗     | ✗       | ✓    |                                                   |
| Completion (keywords / lexical) | ✓     | ✓       | ✓    | Keywords and local syntax fragments               |
| Completion (names / members)    | ~     | ✓       | ✓    | `~` = recovered names only                        |
| Hover                           | ~     | ✓       | ✓    | Parse-only: keyword/syntax hints if any           |
| Definition                      | ✗     | ✓       | ✓    | Requires successful resolve of that name          |
| References                      | ✗     | ✓       | ✓    | Better precision after type when overloads exist  |
| Rename                          | ✗     | ✓       | ✓    | Refuses if resolve is incomplete or ambiguous     |
| Document symbols                | ✓     | ✓       | ✓    | Structure from parse; names refined after resolve |
| Workspace symbols               | ✗     | ✓       | ✓    | Needs index built from resolve-capable packages   |
| Formatting                      | ✓     | ✓       | ✓    | Formatter may work on syntax tree alone           |
| Code actions                    | ~     | ✓       | ✓    | Only actions whose preconditions are met          |
| Semantic tokens                 | ✓     | ✓       | ✓    | Optional; richer legend after resolve/type        |

Legend: **✓** supported at that phase depth; **✗** not available until a deeper phase succeeds; **~** best-effort / degraded.

If only parse succeeds, the server still answers completion/hover/document symbols with degraded content rather than method-not-found. Capabilities stay advertised; results shrink.

### 6.13 Semantic tokens and document state

Semantic tokens are optional. If implemented, the server uses compiler token/symbol classification and advertises a versioned legend. Tokens must be invalidated when the document snapshot changes; clients must be able to request a full refresh before delta encoding is introduced.

### 6.14 Protocol extensions

Aura-specific protocol surface is additive and namespaced under `aura/`.

#### 6.14.1 `aura/status`

Optional notification or request describing analysis state, for example:

- session id / server version;
- current snapshot generation;
- indexing progress (packages done / total);
- focused document phase (parse / resolve / type);
- last error summary (non-sensitive).

Clients must not require `aura/status` for baseline LSP behavior.

#### 6.14.2 Other `aura/*` methods

Future extensions (examples, not MVP commitments):

| Method / capability | Purpose                                   |
| ------------------- | ----------------------------------------- |
| `aura/status`       | Progress and analysis state               |
| `aura/packageGraph` | Debug/visualize resolved packages         |
| `aura/memoryStats`  | Optional diagnostics for large workspaces |

All custom methods use the `aura/` prefix. Experimental methods use `aura/experimental/` or are gated by an explicit capability flag.

#### 6.14.3 Experimental extensions

Experimental APIs:

- are off by default or gated by client capability;
- may change without the same stability guarantees as baseline LSP features;
- must not be required for correctness of standard requests.

#### 6.14.4 Extension versioning policy

- Extensions advertise a version string or capability object at `initialize`.
- Breaking changes to an extension bump that extension’s version and remain behind a capability check.
- Removal of an experimental extension is allowed with release notes; removal of a stabilized extension follows §13.

### 6.15 Configuration and logging

Configuration is read from the client when available and otherwise uses the same safe defaults as the CLI. It includes compiler target/profile, diagnostics verbosity, formatter settings, feature toggles, indexing aggressiveness, and resource limits. The server logs to stderr or the LSP log channel only; stdout is reserved for JSON-RPC. Logs must not contain source contents, secrets, or dependency credentials by default.

Detailed observability is specified in §14.

### 6.16 Performance characteristics

These are **targets** for a warm server on a developer machine, not hard protocol requirements. Implementations should document measured values over time.

| Scenario                                          | Target (MVP intent)                                               |
| ------------------------------------------------- | ----------------------------------------------------------------- |
| Process startup to `initialize` result            | ≤ 500 ms for CLI binary already on PATH                           |
| Workspace load (small, ≤ 50 packages, cold index) | First diagnostics for focused file ≤ 2 s                          |
| Workspace load (large)                            | Focused-file diagnostics first; full index bounded and background |
| Completion latency (warm, small package)          | ≤ 150 ms p95                                                      |
| Hover latency (warm)                              | ≤ 150 ms p95                                                      |
| Definition (warm, same package)                   | ≤ 150 ms p95                                                      |
| Memory (small workspace)                          | Comfortable under ~300 MB RSS exclusive of editor                 |
| Memory (large monorepo)                           | Soft budget configurable; degrade index before OOM                |

Degradation order under pressure: drop background prefetch → shrink dependency index → coarsen workspace symbols → lengthen diagnostic cadence for non-focused files. Interactive completion/hover for the focused file are preserved longest.

### 6.17 Resource management

#### 6.17.1 CPU limits

- Bound worker threads (default: min of CPU count and a configured cap).
- Background work uses lower scheduling priority bands (§6.8).
- Pathological files (huge generated inputs) are time-budgeted per query.

#### 6.17.2 Memory usage

See §6.9.4–6.9.5. The server prefers eviction and partial results over unbounded growth.

#### 6.17.3 Large workspace behavior

For very large workspaces the server:

- indexes workspace members before deep dependency graphs;
- bounds workspace-symbol and references result sets;
- may require explicit config to index all dependencies;
- keeps the focused document’s analysis preferred.

#### 6.17.4 Cancellation budget

Each request has a cooperative cancellation budget:

- client `$/cancelRequest` is honored at phase boundaries;
- obsolete version cancellation is automatic for superseded document work;
- long queries check cancellation between packages/files;
- if cancellation is requested, the server does not later send a success result for that id.

### 6.18 Examples

Launching the server from a package root:

```text
$ aura language-server
```

An editor then sends ordinary LSP messages. The server’s response is conceptually equivalent to:

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "result": {
    "contents": {
      "kind": "markdown",
      "value": "`fun greet(name: String): String`"
    },
    "range": {
      "start": { "line": 3, "character": 4 },
      "end": { "line": 3, "character": 9 }
    }
  }
}
```

### 6.19 Error model / edge cases

- Malformed JSON-RPC receives a protocol error where possible; the process remains alive for recoverable messages.
- Unknown methods return the standard method-not-found error.
- A canceled request produces no late success result; an implementation may return the standard request-canceled error.
- Invalid UTF-8, missing files, duplicate workspace roots, and file URI mismatches are handled as scoped errors.
- Rename refuses edits that would cross package visibility or alter generated/dependency-owned files.
- Formatting is idempotent for a stable snapshot and returns no edits when already formatted.
- Analysis failures are logged with a request id and surfaced as a bounded user-facing message without a Rust panic crossing the protocol boundary.
- Out-of-order document sync is ignored without corrupting other URIs (§6.4.3).

### 6.20 Compatibility & migration

The server follows the LSP version negotiated during `initialize`. Aura-specific behavior is guarded by capability negotiation and server version. Feature additions are backward-compatible; changes to symbol identity, diagnostic codes, or edit safety require a documented compatibility note.

The initial server may coexist with external editor plugins that invoke `aura check`. Plugins can migrate incrementally: diagnostics first, then navigation/completion, while retaining CLI commands for CI and scripts. No source-language syntax is added by this RFC.

Full versioning rules are in §13.

## 7. Open questions

| #   | Question                                   | Options                                                     | Owner            | Status |
| --- | ------------------------------------------ | ----------------------------------------------------------- | ---------------- | ------ |
| 1   | Separate binary or CLI subcommand?         | Subcommand; separate binary; both                           | Tooling          | Open   |
| 2   | Which package sources may be indexed?      | Workspace only; locked dependencies; downloaded source maps | Tooling          | Open   |
| 3   | Should semantic tokens ship in MVP?        | Full tokens; lexical fallback; later                        | Compiler         | Open   |
| 4   | How are generated Aura sources identified? | Manifest metadata; attributes; convention                   | Language/tooling | Open   |
| 5   | Default memory soft limit for monorepos?   | Fixed RSS; fraction of system RAM; unlimited with warn      | Tooling          | Open   |
| 6   | Incremental text sync in MVP?              | Full only; full + incremental                               | Tooling          | Open   |

## 8. Rationale & trade-offs

Sharing the compiler frontend makes the server trustworthy and limits semantic duplication, at the cost of designing compiler queries that tolerate incomplete input. Immutable snapshots and cancellation consume memory and scheduling complexity, but prevent stale or mixed-version responses—failures that are especially confusing in an editor.

Stdio is the conservative default because it integrates with editor process managers and avoids an accidental local network service. A future socket or daemon mode can be added as an explicit opt-in if startup latency becomes measurable.

An explicit feature availability matrix and phase-aware Analysis API make degradation predictable: editors keep working after parse errors, which matches how developers actually type. Separate indexing and request-priority models avoid the common failure mode where a full-project reindex freezes completion.

## 9. Unresolved / future work

- Incremental, persistent analysis across process restarts.
- Full semantic-token deltas and linked editing ranges.
- Cross-package reference indexing with explicit source/version provenance beyond MVP bounds.
- Refactorings beyond rename and compiler-suggested safe fixes.
- Debug Adapter Protocol integration and build/test task discovery.
- Remote workspaces, richer virtual files, and generated-source navigation UX.
- Incremental parser and finer-grained query invalidation.
- Daemon / socket transport as explicit opt-in.

## 10. Security & safety considerations

The server runs with the user’s project permissions and must treat workspace files, configuration, and dependency metadata as untrusted input. It must not execute Aura code, build scripts, macros that require arbitrary code execution, or arbitrary dependency hooks merely to answer an editor request. Network access is disabled by default; dependency fetching, if ever supported, requires an explicit client/CLI action and must reuse lockfile verification from RFC-005.

Workspace edits are constrained to client-owned files and are returned for client review. Logs redact credentials and avoid source leakage. Resource limits, cancellation, and process isolation protect editors from pathological files or dependency graphs.

## 11. Implementation plan

| Phase | Scope                                                                                     | Exit criteria                                                                  |
| ----- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| MVP   | CLI entrypoint, stdio JSON-RPC, workspace snapshots, diagnostics, shutdown/cancel         | An editor can open a package and receive correct diagnostics for unsaved edits |
| v1    | Completion, hover, definition, references, rename, formatting, symbols, safe code actions | Feature corpus passes against golden LSP request/response fixtures             |
| later | Persistent queries, semantic-token deltas, remote/generated sources, DAP                  | Measured startup/latency problem and a reviewed extension contract             |

## 12. Testing strategy

Language server correctness is protocol- and snapshot-oriented. Tests must not depend on a particular editor UI.

| Kind                       | Purpose                                                                     |
| -------------------------- | --------------------------------------------------------------------------- |
| LSP conformance tests      | Lifecycle, capabilities, document sync, cancellation, basic requests        |
| Golden protocol tests      | Checked-in JSON-RPC request/response (or transcript) fixtures per feature   |
| Snapshot tests             | Analysis API outputs for fixed workspace fixtures (diagnostics, symbols)    |
| Cancellation tests         | Superseded versions and `$/cancelRequest` never yield late success          |
| Out-of-order sync tests    | didChange before didOpen, stale versions, duplicate roots                   |
| Fuzz testing               | Malformed JSON-RPC frames, random edits, huge documents (process stays up)  |
| Large workspace benchmarks | Startup, focused diagnostics, completion p95, RSS under load                |
| Parity tests               | Same snapshot → server diagnostics match `aura check` where phases complete |

CI should run conformance + golden + snapshot suites on every change; large benchmarks may be nightly or manual with recorded budgets.

## 13. Versioning policy

### 13.1 LSP compatibility

- The server negotiates LSP via `initialize` and implements a documented minimum LSP version.
- Newer LSP features are optional capabilities; older clients keep working.
- The server MUST NOT require non-standard client extensions for baseline features.

### 13.2 Compiler compatibility

- The language server version is tied to the toolchain version (`aura --version` / server `ServerInfo`).
- Mixing a server binary with a different compiler library build is unsupported; the CLI ships them together.
- Diagnostic codes and symbol identity stability follow compiler RFCs; breaking changes need release notes.

### 13.3 Extension compatibility

- `aura/*` extensions version independently via capabilities (§6.14.4).
- Experimental extensions may break between minor toolchain releases.
- Stabilized extensions follow the same bar as baseline features for removal.

### 13.4 Breaking changes

Breaking changes include:

- removing an advertised capability without negotiation fallback;
- changing rename/edit safety so previously accepted edits become silently wrong;
- changing diagnostic code meanings in place;
- altering snapshot version attribution so stale results appear current.

Such changes require a documented compatibility note, capability or version gate when possible, and changelog entry.

## 14. Observability

### 14.1 Logging

- Logs go to stderr and/or LSP log messages; never stdout.
- Default level is suitable for field debugging without source dumps.
- Request ids, URI paths (not full file bodies), durations, and phase outcomes are loggable.
- Source text, tokens, secrets, and credentials are redacted by default.

### 14.2 Trace

When the client enables LSP trace (`verbose` / `message`), the server emits protocol-level traces. An internal analysis trace (query names, cache hit/miss, cancel reasons) may be enabled via configuration for developer builds.

### 14.3 Metrics

Implementations SHOULD track at least:

- request counts and latencies by method;
- cancel and timeout rates;
- index size and rebuild counts;
- cache hit rates;
- RSS / memory pressure events.

Metrics may be exposed via log summaries, `aura/status`, or a debug-only endpoint; they are not required on the wire for MVP.

### 14.4 Telemetry

Optional product telemetry, if ever enabled:

- is **off by default**;
- requires explicit user opt-in;
- sends no source code, file contents, or secrets;
- is documented separately from this protocol contract.

This RFC does not require telemetry.

### 14.5 Debug mode

A debug or developer mode may:

- include richer `aura/status`;
- dump package graph and memory stats;
- relax redaction for local-only reproduction (still never on stdout JSON-RPC).

Debug mode must not be required for normal editor use.

## 15. Platform support

| Platform | Support     | Notes                               |
| -------- | ----------- | ----------------------------------- |
| macOS    | First-class | Default developer target            |
| Linux    | First-class | CI reference                        |
| Windows  | First-class | Path and URI normalization critical |

### 15.1 URI normalization

- Prefer `file:` URIs with consistent percent-encoding.
- Normalize path case according to platform rules without breaking case-sensitive volumes.
- Treat `file:///C:/...` and equivalent forms deterministically on Windows.
- Document identity is the normalized URI string used as the snapshot key.

### 15.2 File watching differences

- Prefer client `workspace/didChangeWatchedFiles` when available.
- Server-side watching, if used, must account for OS limits (e.g. `inotify` caps on Linux) and fall back to coarser directory watches or client-driven sync.
- Symlinks and case-only renames are handled without duplicate package membership when possible; remaining ambiguities produce diagnostics.

## 16. Known limitations

The following are accepted limitations for early implementations unless a later phase removes them. They are not protocol bugs when documented and reflected in capabilities.

| Limitation                           | Implication                                                        |
| ------------------------------------ | ------------------------------------------------------------------ |
| No incremental parser (initially)    | Edits may reparse larger regions than the changed span             |
| No semantic token deltas             | Full token refresh only until deltas ship                          |
| Workspace symbols bounded            | Very large projects return truncated, deterministic sets           |
| Rename only inside workspace         | Dependency / generated symbols are rejected                        |
| References default to workspace      | Cross-dependency search opt-in and bounded                         |
| Full document sync only (MVP option) | More bandwidth until incremental sync is enabled                   |
| No build-script execution            | Codegen’d sources may be missing until produced outside the server |
| No DAP / debug in this RFC           | Debugging is out of scope here                                     |
| Best-effort under parse errors       | Resolve/type features degrade per §6.12                            |
| Single process session               | One workspace negotiation per server process                       |

## 17. References

- [RFC-001: Language Specification](RFC-001-language-specification.md)
- [RFC-002: Type System](RFC-002-type-system.md)
- [RFC-004: Compiler Architecture](RFC-004-compiler-architecture.md)
- [RFC-005: Package Manager](RFC-005-package-manager.md)
- [RFC-008: Build System](RFC-008-build-system.md)
- [RFC-012: CLI](RFC-012-cli.md)
- [Language Server Protocol specification](https://microsoft.github.io/language-server-protocol/)

---

## Changelog

| Date       | Author            | Change                                                                                                                                                                                                               |
| ---------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-07-22 | Aura contributors | Initial draft                                                                                                                                                                                                        |
| 2026-07-22 | Aura contributors | Expand design: compiler integration, sync, workspace, indexing, scheduling, caching, feature matrix, extensions, performance, resources; add testing, versioning, observability, platform support, known limitations |
