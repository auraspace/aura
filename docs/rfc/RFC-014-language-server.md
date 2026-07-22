# RFC-014: Language Server

| Field             | Value                                       |
| ----------------- | ------------------------------------------- |
| **RFC**           | 014                                         |
| **Title**         | Language Server                             |
| **Status**        | Draft                                       |
| **Layer**         | Toolchain                                   |
| **Authors**       | Aura contributors                           |
| **Created**       | 2026-07-22                                  |
| **Updated**       | 2026-07-22                                  |
| **Estimate**      | 30–50 pages                                 |
| **Depends**       | RFC-001, RFC-002, RFC-004, RFC-008, RFC-012 |
| **Blocks**        | —                                           |
| **Supersedes**    | —                                           |
| **Superseded by** | —                                           |

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
3. `didOpen`, `didChange`, and `didClose` update an in-memory document store. Each document has a monotonically increasing client version; stale changes are rejected or ignored without corrupting the snapshot.
4. Workspace configuration is loaded from `aura.toml` and lockfiles using the same resolution rules as RFC-005 and RFC-008. Unknown or unavailable dependencies produce actionable diagnostics, not a server crash.
5. `shutdown` ends analysis; `exit` terminates the process according to LSP.

The open document is authoritative over its on-disk counterpart. Files not opened by the client are read from disk through a workspace file abstraction. URI/path conversion, encoding, and file identity must be deterministic across platforms.

### 6.3 Analysis snapshots and scheduling

Every request observes one immutable workspace snapshot. A new edit can invalidate queued work, but it must not mutate data being read by an in-flight request. The scheduler:

- prioritizes diagnostics for the most recently changed document;
- cancels obsolete requests when the client supplies a cancellation token;
- bounds parallelism and memory usage;
- returns an empty or partial result when a feature cannot be computed yet;
- never reports results for an older document version as if they described the latest version.

The first implementation may rebuild the affected package or module. Incremental query caching is an optimization and must not change observable semantics. Cache keys include document content/version, compiler configuration, dependency lock state, and toolchain version.

### 6.4 Diagnostics

Diagnostics use the shared `aura-diagnostics` model and are converted to LSP ranges only at the protocol boundary. The server publishes lexical, parse, name-resolution, and type diagnostics with severity, source, stable code, related information, and notes where the client supports them.

Diagnostics are published per URI and replaced atomically for the current open-document snapshot. A parse error should retain any symbols that can still be recovered, so completion and navigation remain useful. The server must distinguish an unavailable dependency, an invalid source edit, and an internal server failure in logs and client-visible messages.

### 6.5 Baseline language features

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

### 6.6 Semantic tokens and document state

Semantic tokens are optional. If implemented, the server uses compiler token/symbol classification and advertises a versioned legend. Tokens must be invalidated when the document snapshot changes; clients must be able to request a full refresh before delta encoding is introduced.

The server may expose an Aura extension named `aura/status` for progress and analysis state. Extensions are additive, namespaced, and never required for baseline LSP behavior.

### 6.7 Configuration and logging

Configuration is read from the client when available and otherwise uses the same safe defaults as the CLI. It includes compiler target/profile, diagnostics verbosity, formatter settings, and feature toggles. The server logs to stderr or the LSP log channel only; stdout is reserved for JSON-RPC. Logs must not contain source contents, secrets, or dependency credentials by default.

### 6.8 Examples

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

### 6.9 Error model / edge cases

- Malformed JSON-RPC receives a protocol error where possible; the process remains alive for recoverable messages.
- Unknown methods return the standard method-not-found error.
- A canceled request produces no late success result; an implementation may return the standard request-canceled error.
- Invalid UTF-8, missing files, duplicate workspace roots, and file URI mismatches are handled as scoped errors.
- Rename refuses edits that would cross package visibility or alter generated/dependency-owned files.
- Formatting is idempotent for a stable snapshot and returns no edits when already formatted.
- Analysis failures are logged with a request id and surfaced as a bounded user-facing message without a Rust panic crossing the protocol boundary.

### 6.10 Compatibility & migration

The server follows the LSP version negotiated during `initialize`. Aura-specific behavior is guarded by capability negotiation and server version. Feature additions are backward-compatible; changes to symbol identity, diagnostic codes, or edit safety require a documented compatibility note.

The initial server may coexist with external editor plugins that invoke `aura check`. Plugins can migrate incrementally: diagnostics first, then navigation/completion, while retaining CLI commands for CI and scripts. No source-language syntax is added by this RFC.

## 7. Open questions

| #   | Question                                   | Options                                                     | Owner            | Status |
| --- | ------------------------------------------ | ----------------------------------------------------------- | ---------------- | ------ |
| 1   | Separate binary or CLI subcommand?         | Subcommand; separate binary; both                           | Tooling          | Open   |
| 2   | Which package sources may be indexed?      | Workspace only; locked dependencies; downloaded source maps | Tooling          | Open   |
| 3   | Should semantic tokens ship in MVP?        | Full tokens; lexical fallback; later                        | Compiler         | Open   |
| 4   | How are generated Aura sources identified? | Manifest metadata; attributes; convention                   | Language/tooling | Open   |

## 8. Rationale & trade-offs

Sharing the compiler frontend makes the server trustworthy and limits semantic duplication, at the cost of designing compiler queries that tolerate incomplete input. Immutable snapshots and cancellation consume memory and scheduling complexity, but prevent stale or mixed-version responses—failures that are especially confusing in an editor.

Stdio is the conservative default because it integrates with editor process managers and avoids an accidental local network service. A future socket or daemon mode can be added as an explicit opt-in if startup latency becomes measurable.

## 9. Unresolved / future work

- Incremental, persistent analysis across process restarts.
- Full semantic-token deltas and linked editing ranges.
- Cross-package reference indexing with explicit source/version provenance.
- Refactorings beyond rename and compiler-suggested safe fixes.
- Debug Adapter Protocol integration and build/test task discovery.
- Remote workspaces, virtual files, and generated-source navigation.

## 10. Security & safety considerations

The server runs with the user’s project permissions and must treat workspace files, configuration, and dependency metadata as untrusted input. It must not execute Aura code, build scripts, macros, or arbitrary dependency hooks merely to answer an editor request. Network access is disabled by default; dependency fetching, if ever supported, requires an explicit client/CLI action and must reuse lockfile verification from RFC-005.

Workspace edits are constrained to client-owned files and are returned for client review. Logs redact credentials and avoid source leakage. Resource limits, cancellation, and process isolation protect editors from pathological files or dependency graphs.

## 11. Implementation plan

| Phase | Scope                                                                                     | Exit criteria                                                                  |
| ----- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| MVP   | CLI entrypoint, stdio JSON-RPC, workspace snapshots, diagnostics, shutdown/cancel         | An editor can open a package and receive correct diagnostics for unsaved edits |
| v1    | Completion, hover, definition, references, rename, formatting, symbols, safe code actions | Feature corpus passes against golden LSP request/response fixtures             |
| later | Persistent queries, semantic-token deltas, remote/generated sources, DAP                  | Measured startup/latency problem and a reviewed extension contract             |

## 12. References

- [RFC-001: Language Specification](RFC-001-language-specification.md)
- [RFC-002: Type System](RFC-002-type-system.md)
- [RFC-004: Compiler Architecture](RFC-004-compiler-architecture.md)
- [RFC-008: Build System](RFC-008-build-system.md)
- [RFC-012: CLI](RFC-012-cli.md)
- [Language Server Protocol specification](https://microsoft.github.io/language-server-protocol/)

---

## Changelog

| Date       | Author            | Change        |
| ---------- | ----------------- | ------------- |
| 2026-07-22 | Aura contributors | Initial draft |
