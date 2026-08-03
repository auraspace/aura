# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Resolved

### IR-001 compiler architecture migration (resolved 2026-08-03)

- Area: aura-ir, alpha C backend
- Historical issue: the alpha C compatibility path originally shared source
  lowering state with the backend-neutral pipeline.
- Progress (2026-08-03): the async CFG node/frame model moved to aura-ir and
  validates all state edges before C emission. MIR validation and independent
  async model fixtures now run without compiling C.
- Progress (2026-08-03): supported linear async bodies now lower into typed
  MIR during CheckedFile-to-LoweredProgram conversion; unsupported bodies are
  recorded explicitly in async_mir_unlowered rather than silently treated as
  complete.
- Progress (2026-08-03): the typed async lowerer now emits SwitchInt CFG
  edges for simple conditional branches and validates their targets without
  involving C syntax.
- Progress (2026-08-03): the backend trait/driver are public and receive
  LoweredProgram, so a future LLVM/Cranelift implementation can be added
  without coupling dispatch to C; only CBackend uses c_source.
- Progress (2026-08-03): typed MIR binding lowering now emits Move/Clone
  statements according to the ownership plan, with a semantic fixture proving
  an owned String local is moved without inspecting generated C.
- Progress (2026-08-03): call-based await operands now become typed task
  locals plus Await resume edges in MIR, with no C expression fragments.
- Progress (2026-08-03): aura-analysis now publishes CheckedIr alongside
  AST and CheckedFile, making the target-neutral boundary available to CLI,
  LSP, and future backend hosts.
- Progress (2026-08-03): MIR CFG lowering also models empty while loops with
  explicit loop back-edges and exit edges; complex loop bodies remain
  explicitly listed in async_mir_unlowered.
- Progress (2026-08-03): checked IR now extracts typed try/catch/finally
  regions and throw presence before backend emission; exception fixtures do
  not inspect setjmp/longjmp output.
- Progress (2026-08-03): simple async throw/catch bodies now lower to typed
  EnterTry and Throw handler edges in MIR; complex catch/finally shapes remain
  explicitly rejected by the strict backend gate.
- Progress (2026-08-03): generic monomorphization requests are collected and
  deterministically normalized in aura-ir before backend selection, with
  semantic coverage for generic function instantiation.
- Progress (2026-08-03): backend options now name LLVM and Cranelift as
  reserved backend identities; validation rejects them until their MIR
  implementations are added instead of silently routing them through C.
- Progress (2026-08-03): backend dispatch rejects incomplete MIR by default;
  only CBackend explicitly opts into the alpha compatibility bridge.
- Progress (2026-08-03): driver coverage proves a strict backend is rejected
  before emission when an async body remains outside the current MIR subset.
- Progress (2026-08-03): foreign library/link kind metadata is now lowered
  into `ForeignLibraryIr`; C linker argument construction no longer reads
  `CheckedFile.ast.foreign_functions`, and the unused source-file accessor was
  removed from the backend-neutral program API.
- Progress (2026-08-03): MIR rvalues now represent null, unary, and binary
  operations with backend-neutral operands; recursive operand lowering and
  validation are covered without asserting on generated C syntax.
- Progress (2026-08-03): the common lowering API now accepts an effect and is
  shared by supported synchronous and async bodies; `FunctionIr.body` and
  `function_mir_unlowered` make partial sync coverage explicit instead of
  silently treating C-only statement lowering as complete.
- Progress (2026-08-03): validated MIR now materializes a separate,
  backend-neutral state machine with successor edges, suspension metadata,
  and frame locals; state-machine construction is independently tested.
- Progress (2026-08-03): side-effecting call statements now lower to a
  validated MIR `Evaluate` operation and preserve their effect without C
  syntax; pure-body coverage is tested through the same lowering API.
- Progress (2026-08-03): C-fragment `AsyncCfgNode`/frame compatibility types
  were moved out of aura-ir into `aura-codegen::async_compat`; aura-ir now
  exposes only the validated MIR-derived state-machine contract.
- Progress (2026-08-03): backend compilation is now an optional capability;
  the public backend contract supplies a safe default error, so a future
  MIR-only LLVM/Cranelift backend need not accept C runtime/compiler inputs.
- Progress (2026-08-03): the C backend now receives `LoweredProgram` through
  `emit_c_with_program` and renders supported primitive sync function bodies
  from `FunctionIr.body`; unsupported shapes are explicitly gated back to the
  alpha compatibility emitter. The renderer and full translation-unit path
  have independent coverage.
- Progress (2026-08-03): the MIR renderer preserves `std.*` intrinsic lowering
  until those runtime operations have explicit IR variants; regression tests
  cover signal and error-kind intrinsics.
- Progress (2026-08-03): the same MIR renderer now supplies supported
  no-suspension async body functions inside the C task wrapper; await-bearing
  or non-primitive shapes remain explicit compatibility fallbacks.
- Progress (2026-08-03): supported synchronous function bodies now publish
  the same typed MIR in `FunctionIr.body` with explicit `function_mir_unlowered`
  coverage, so strict future backends can gate on all function bodies rather
  than only async names.
- Progress (2026-08-03): the primitive C renderer now consumes only
  `FunctionIr`/MIR for supported synchronous functions and no longer receives
  `CheckedFile`; it emits validated multi-block branch CFGs as labels and
  gotos, with source-independent MIR coverage.
- Progress (2026-08-03): branch lowering now supports primitive local bindings
  and expression statements before a terminal return, with branch-local MIR
  scopes and collision-safe local names.
- Progress (2026-08-03): primitive MIR ownership actions (`Move`, `Clone`,
  `Drop`) are now consumed by the C MIR renderer; ownership decisions remain
  represented as MIR actions rather than being inferred again from C syntax.
- Progress (2026-08-03): MIR `Unreachable` emission uses standard C `abort()`
  rather than a compiler-specific builtin, keeping the migrated renderer
  portable across conforming C toolchains.
- Progress (2026-08-03): generic instantiation IR now preserves the owner kind
  (function, async function, class, enum, interface, or method) instead of
  encoding that distinction only in backend-side AST scans.
- Progress (2026-08-03): MIR call rvalues now carry a neutral `CallTarget`
  containing declaring package, explicit/method type arguments, constructor and
  variant metadata; backends no longer need to reconstruct call identity from
  an AST span.
- Progress (2026-08-03): typed exception regions now carry the catch payload
  type in MIR `EnterTry`; catch dispatch no longer has to rediscover that
  semantic fact from C-emitter syntax.
- Progress (2026-08-03): transitive closed generic-function discovery now runs
  in `aura-ir::generics` and feeds the C alpha prototype/body work list from
  `CheckedIr`; the C emitter no longer contains the transitive AST scan.
- Progress (2026-08-03): generic free-function instances are now closed and
  lowered to concrete `FunctionIr` MIR in `aura-ir`; generic async instances
  likewise publish concrete MIR. C uses those bodies whenever the common
  renderer supports them, while retaining its explicit alpha fallback for
  unsupported shapes.
- Progress (2026-08-03): ordinary call rvalues and side-effecting call
  statements now lower through MIR and the C primitive renderer can emit
  user calls with neutral `CallTarget` metadata; `std.*` intrinsics remain
  explicitly gated until they have dedicated IR operations (including the
  compiler-generated `gc_collect`/`gc_mark` operations).
- Progress (2026-08-03): closed generic async bodies now also publish
  validated state-machine records, keeping suspension/frame topology visible
  to future non-C backends rather than reconstructing it from source.
- Progress (2026-08-03): synchronous throwing regions with an empty `finally`
  now lower to MIR `EnterTry`/handler/finally CFG edges; exception topology is
  represented before C emission instead of being inferred only by the C
  compatibility emitter.
- Progress (2026-08-03): MIR-derived suspension states now publish explicit
  ownership actions for locals crossing an await (for example `Clone` for an
  owned String), so future frame backends do not need to recompute lifetime
  policy from C expressions.
- Progress (2026-08-03): generic `TypeRef` substitution used by async class
  method compatibility lowering now lives in `aura-ir::generic_lowering`; the
  C class emitter no longer owns that recursive type substitution helper.
- Progress (2026-08-03): the C spawn compatibility path also consumes the
  shared IR type-reference conversion; its duplicate `Ty`→`TypeRef` mapper
  was removed from `emit.rs`.
- Progress (2026-08-03): zero-argument `gc_collect()` now lowers to the
  backend-neutral `Rvalue::Intrinsic::GcCollect` operation and has a MIR-only
  semantic test. The C MIR renderer rejects this operation until an explicit
  runtime capability is added, preventing accidental free-function ABI output.
- Progress (2026-08-03): discarded async expressions such as `await tick()`
  now lower to typed `Await` terminators with explicit result locals and
  separate resume edges, including sequential awaits; this removes another
  async shape that previously required C compatibility lowering.
- Progress (2026-08-03): `join` and `cancel` now lower to validated neutral
  `AsyncOp` MIR values with explicit handle operands. The C MIR renderer rejects
  these until runtime capability mapping exists, so it cannot infer a C ABI
  from async source syntax.
- Progress (2026-08-03): channel create/send/receive/close expressions now
  lower to the same neutral `AsyncOp` contract with typed channel element
  metadata and explicit operands; channel runtime ABI selection remains a
  declared C-alpha capability gap.
- Progress (2026-08-03): capture-free `spawn { ... }` now embeds a validated
  nested async `MirBody` in neutral `AsyncOp::Spawn`; captured spawn frames and
  C runtime capability mapping were then split out as explicit capability
  boundaries rather than reconstructed from source syntax.
- Progress (2026-08-03): spawn capture discovery now closes over checked outer
  locals deterministically, materializes them as nested MIR parameters, and
  records each boundary ownership action in `SpawnCapture`; shadowed locals are
  excluded from the capture set.
- Progress (2026-08-03): the public backend contract now exposes explicit
  `BackendCapabilities` for complete MIR, alpha source, C runtime, and native
  compilation requirements. The legacy partial-MIR boolean remains only as a
  compatibility shim, while driver gating uses the capability contract.
- Progress (2026-08-03): nested `spawn` MIR bodies are recursively converted
  into published state-machine records in `CheckedIr`, so future backends can
  consume scheduler topology without traversing source or re-deriving frames.
- Progress (2026-08-03): generated C now carries an explicit alpha-backend
  marker documenting that any source compatibility lowering is a bounded
  fallback for unsupported MIR capabilities, not the backend-neutral contract.
- Progress (2026-08-03): general `throw` statements now lower arbitrary
  backend-neutral expressions through temporary MIR places and materialize
  ownership cleanup before the `Throw` terminator, including branch bodies.
- Progress (2026-08-03): pure expression-form `if` branches now lower to a
  typed MIR `Select` value and the primitive C renderer emits the same merge
  from MIR. Effectful, multi-statement, or await-bearing expression branches
  remain explicit CFG-lowering work rather than being misrepresented as eager
  C expressions.
- Progress (2026-08-03): force-unwrap and type-test expressions now have
  explicit neutral MIR operations (`Unwrap` and `TypeTest`) with checked target
  type metadata; the C alpha renderer rejects them until runtime/type-layout
  capabilities are declared.
- Progress (2026-08-03): `Backend::compile_ir` and `BackendBuildOptions` now
  provide a native artifact boundary without `runtime_c`, C compiler, or C
  source inputs; the legacy `compile` method is reserved for the alpha C path.
- Progress (2026-08-03): native backend artifacts can now be constructed with
  `Artifact::from_backend` and a neutral identity that has no `RuntimeAbi`;
  LLVM/Cranelift preparation no longer requires manufacturing a C runtime
  identity merely to report a produced artifact.
- Progress (2026-08-03): direct `return await expr` normalization now belongs
  to `aura-ir::lowering` and has a backend-independent test; C only consumes
  the normalized declaration for its alpha rendering path.
- Progress (2026-08-03): generic free-function and generic async instances
  that cannot yet be closed/lowered are now published in explicit unlowered
  MIR lists. Strict backends therefore reject them instead of treating a
  silently omitted `filter_map` result as complete MIR.
- Progress (2026-08-03): C alpha output now lists the deterministic symbols
  using compatibility fallback lowering, making the remaining source bridge
  visible in generated artifacts and review logs.
- Progress (2026-08-03): the neutral MIR driver now has a repeated-lowering
  equality regression, covering stable block/state serialization rather than
  only artifact identity.
- Progress (2026-08-03): generic AST substitution is documented as a
  frontend Checked IR materialization step, not a temporary backend adapter;
  C/LLVM/Cranelift backends consume the closed result only.
- Progress (2026-08-03): non-empty `while` bodies containing the supported
  linear statement subset now lower to MIR loop CFGs and the C renderer emits
  them from MIR; loop shape and renderer coverage are independent tests.
- Progress (2026-08-03): simple exclusive/inclusive integer ranges now lower
  to typed counter/compare/increment MIR CFGs, with C rendering covered from
  MIR rather than the range AST.
- Progress (2026-08-03): a linear `while` body containing one awaited call now
  lowers to an explicit MIR `Await` resume edge and a state-machine loop-back
  edge; the suspension topology is tested without C generation.
- Progress (2026-08-03): the same async loop lowering now supports an awaited
  value binding followed by a resumed linear statement, preserving the value
  local and call effect in the post-resume MIR block.
- Progress (2026-08-03): range loops reuse the same await/resume lowering and
  route the resumed block through an explicit counter-increment CFG node.
- Progress (2026-08-03): async `try { val x = await call(); throw x } catch`
  now lowers to MIR with both suspension unwind and post-resume typed throw
  edges, making this exception topology backend-neutral.
- Progress (2026-08-03): MIR now materializes lexical `Drop` operations at
  ordinary return, throw, branch-join, and implicit scope exits according to
  the ownership plan; returned places are excluded so ownership transfer is
  explicit. Semantic tests verify cleanup without inspecting generated C.
- Progress (2026-08-03): local assignment statements now lower to typed MIR
  replacement actions, including destination cleanup plus `Move`, `Clone`, or
  `Retain` according to the checked ownership plan. Primitive and owned-string
  assignment fixtures validate the IR without inspecting generated C.
- Progress (2026-08-03): the same assignment path is now used inside lowered
  branch and loop CFG bodies, removing another source-syntax fallback for
  primitive control-flow functions.
- Progress (2026-08-03): `break` and `continue` now lower to validated loop
  CFG edges, and loop exits clean only locals introduced by the loop body so
  outer ownership remains live after the edge. MIR tests cover both edges and
  the outer-local cleanup invariant.
- Progress (2026-08-03): unit-payload enum matches now lower to a typed
  `VariantTag` rvalue and multi-target `SwitchTag` terminator. State-machine
  successor construction and validation cover the new tag-switch contract;
  payload bindings remain an explicit unsupported MIR shape.
- Progress (2026-08-03): non-generic primitive enum payload bindings initially
  lowered to typed variant extraction and arm-local MIR bindings; this
  was superseded by the ownership-aware extraction operation below.
- Progress (2026-08-03): `ExtractVariantField` now carries the checked
  ownership action and supports generic enum type-parameter substitution for
  concrete primitive/String payloads.
- Progress (2026-08-03): recursive checked `TypeRef` mapping now covers
  nullable values, arrays, task/channel handles, and declared class/enum/
  interface applications for enum payload extraction. Array payload fixtures
  validate owned `Move` extraction without C syntax.
- Progress (2026-08-03): Array and String `for-in` now lower to typed MIR
  `Length`/`LoadIndex` counter loops with explicit condition, increment,
  ownership action, and break/continue edges. Awaiting/nested iterator protocol
  bodies remain explicitly outside this slice.
- Progress (2026-08-03): branch lowering now preserves a lexical scope across
  an awaited local binding by splitting the branch at `Await` and recursively
  lowering the post-resume tail. This enables awaited bindings in conditional
  branches and fixes the CFG invariant that incorrectly rejected a return after
  a branch merely because continuation blocks had already been allocated.
- Progress (2026-08-03): nested `if` statements inside branch and loop bodies
  now lower their post-join tail instead of requiring the conditional to be the
  final statement. Nested CFG coverage verifies the join remains backend-neutral.
- Progress (2026-08-03): formal interface iterables and duck-typed class
  iterables with `len()`/`get(Int)` now lower to receiver-aware neutral method
  calls, including typed element results and explicit ownership transfer into
  the loop binding. Class `len` fields, richer iterator objects, and async
  protocol method calls remain explicit follow-up shapes; class `len` fields are
  represented by a neutral `Field` rvalue. The alpha C primitive
  renderer rejects the protocol marker and routes these bodies through its
  explicitly marked compatibility fallback until receiver dispatch has a C ABI
  independent implementation.
- Progress (2026-08-03): the driver now exposes `BackendOptions` and routes
  emission through `Backend::emit_ir`; the legacy C-shaped `emit` method remains
  only as a compatibility shim while existing alpha integrations migrate.
- Progress (2026-08-03): generic class-method closure now materializes both
  async and synchronous monomorphized method bodies into `aura-ir` MIR; async
  method state machines and strict unlowered-method lists are published as
  backend inputs. Receiver-call lowering now represents the receiver as the
  first neutral call operand, and field reads (`this.item`) use `Field` rvalues;
  C remains the alpha fallback for unsupported receiver ABIs.
  Resolution: Checked IR now owns semantic facts, generic closure, ownership
  plans, exception regions, typed MIR, and MIR-derived async state machines.
  The public backend contract rejects incomplete MIR by default, and the C-only
  source view is explicitly isolated as an alpha compatibility input. The
  backend-neutral `MirBackend` test verifies that emission works without C
  source, runtime input, or a system compiler. LLVM and Cranelift can therefore
  implement the same contract without inheriting the C compatibility path.

## Open

### API-001 alpha std crypto/reflection placeholders (2026-07-31)

- Area: `std.crypto`, `std.reflect`
- Symptom: the alpha API and package resolution are locked, but cryptographic,
  TLS, and reflection operations intentionally throw placeholder errors.
- Why deferred: secure crypto backend selection, key ownership, capability
  policy, and metadata retention need a dedicated implementation/RFC pass.
- Next step: implement a verified platform crypto provider and opt-in metadata
  emission without changing the locked signatures.

### API-002 alpha protocol placeholders (2026-07-31)

- Area: `std.tls`, `std.udp`, `std.websocket`, `std.compress`, `std.multipart`
- Symptom: package contracts and typed values are locked, but all transport,
  framing, compression, and multipart operations fail explicitly.
- Why deferred: each backend needs independent capability limits, async
  ownership rules, parser hardening, and sanitizer coverage.
- Next step: implement TLS/UDP foundations first, then WebSocket and bounded
  streaming adapters, without changing the locked public signatures.

### API-003 alpha data API placeholders (2026-07-31)

- Area: `std.json.Value`, `std.collections.List<T>` follow-up semantics
- Symptom: JSON tree traversal, cloning, policy-aware parsing, and typed
  mapping remain placeholders; List iterator invalidation and clone semantics
  are not yet specified in the implementation.
- Why deferred: owned JSON trees, duplicate-key policy, byte/depth enforcement,
  generic decode derives, and collection snapshot rules need a coherent
  ownership pass. The initial Array-backed List storage and generic `map<R>`
  are implemented.
- Next step: implement JSON limits/policy first, then finalize List clone and
  iterator behavior without changing the locked source names.

### API-004 RFC contract placeholders (2026-07-31)

- Area: `std.reflect`, `std.test`, and shared errors
- Symptom: canonical RFC names/signatures remain reserved for reflection,
  package-specific error enums, assertion execution, and metadata emission.
- Why deferred: namespace-qualified enum variants and full runtime
  metadata/ownership support are still incomplete. Generic class methods and
  static class members were implemented for the List API in this change.
- Next step: implement the remaining compiler/runtime capabilities without
  changing the locked source contracts.

### API-006 compiler/runtime/tooling boundary inventory (2026-07-31, updated)

- Area: RFC-001/002/004/005/006/008/012/013/014 surfaces outside `std` source APIs
- Symptom: user macro expansion/sandboxing, concurrent tracing GC,
  cross-target build/sysroot delivery, release self-update, and full LSP
  protocol behavior are described by RFCs but are not all implemented.
- Progress: attributes are retained as typed sema metadata, built-in and
  registered `UserDerive`/`UserMacro` AST expansions record their phase and source origin, and
  the lexer now exposes delimiter-aware token trees with span-preserving
  flattening, metavariable matching, and template substitution primitives for
  RFC-010 expansion; the parser now expands top-level function-like rules
  before AST construction with a bounded recursion limit. A versioned binary
  plugin request/response ABI now has explicit UTF-8 framing, ABI rejection,
  output limits, timeout handling, and fail-closed OS sandbox selection.
  Generated C exposes a versioned
  Binary/Runtime metadata table. Built-in
  `Equals`, `HashCode`, `Debug`, and `ToString` derives are compiler-generated
  and ownership-checked.
- Why the remaining macro boundary is deferred: token-tree repetition now
  supports multiple captures per item, nested one-level repetition composition,
  and both `*`/`+` operators. Binding/item gensym, lexical nested-scope
  hygiene, and duplicate export checks are implemented. Root package plugin
  discovery and build invocation are now wired through `[macro_plugins]` and
  the CLI's check/emit/build/test paths.
  The runner refuses hosts without a supported OS sandbox instead of weakening
  RFC-010's supply-chain and capability contract. Concurrent tracing collection
  still needs write barriers and precise stack maps beyond the executor-safe
  STW collector; cross-target build/sysroot, self-update, and full LSP remain
  separate tooling/distribution work.
- Progress: the compiler now owns the post-plugin expansion boundary: generated
  source is parsed and merged before typecheck, package identity changes are
  rejected, and expansion metadata is retained. Plugin stdout/stderr are
  drained concurrently under the configured output cap, preventing a noisy
  process from deadlocking the host while it waits for termination.
- Progress (2026-08-01): declarative macro exports are now indexed by name while
  loading package sources and dependency graphs. Duplicate names fail closed
  with a deterministic diagnostic instead of silently selecting the first
  definition by filesystem/dependency load order.
- Progress (2026-08-01): declarative macro templates now gensym identifiers at
  declaration sites (`val`, `var`, and item declarations) per invocation while
  preserving metavariable capture spellings. Parser coverage proves a macro
  local cannot capture or overwrite a caller local with the same source name.
- Progress (2026-08-02): hygiene expansion now carries a lexical token-tree
  scope through nested groups and function parameters. Shadowed declaration
  sites receive distinct invocation-local names while metavariable captures
  remain call-site identifiers; parser coverage verifies nested shadowing.
- Progress (2026-08-01): dependency manifests that declare procedural plugins
  now fail closed during graph loading instead of silently dropping executable
  provenance. Only root `[macro_plugins]` entries can be selected, and the
  loader regression verifies the deterministic diagnostic.
- Progress (2026-08-01): root plugin paths are restricted to package-relative
  paths without parent traversal, preventing a manifest from selecting an
  executable outside its declared package root; loader coverage verifies the
  escape-path rejection.
- Progress (2026-08-01): Linux procedural plugins now execute through
  `bubblewrap` with a private network namespace, read-only source/plugin and
  runtime-library binds, a private `/tmp`, and cleared environment. The stable
  request encoder is also exported through `aura-analysis` for plugin hosts.
- Progress (2026-08-02): the stable request/response protocol now caps every
  UTF-8 field at 16 MiB, rejects oversized fields before plugin execution or
  decoder allocation, and uses checked `u32` framing instead of truncating
  lengths. Protocol regressions cover both malformed oversized input and the
  encoder boundary.
- Progress (2026-08-01): `aura.lock` now records root plugin pins as
  `macro_plugin.<Name>` entries with package-relative paths and SHA-256 checksums.
  Writable CLI loads create pins; read-only loads require them, and every load
  re-hashes the executable so replacement binaries fail closed. A package test
  covers pin creation, matching verification, and post-pin mutation.
- Next step: define explicit unhygienic spans and expansion inspection while
  keeping the ABI version unchanged; dependency plugin provenance is now
  represented by verified root lock pins and remains fail-closed for
  dependency-owned executables.

### NET-001 endpoint parsing is synchronous and string-based (2026-07-31)

- Area: `std.net`, POSIX runtime TCP transport
- Symptom: endpoint strings are parsed by the synchronous runtime bridge and
  hostname resolution uses `getaddrinfo()` before the task is scheduled; there
  is no typed endpoint value or asynchronous resolver contract yet.
- Why deferred: the alpha transport needs a small usable bind/connect surface
  first, while DNS caching, cancellation, and resolver error typing belong to a
  broader networking design.
- Next step: introduce a validated endpoint type and move potentially blocking
  name resolution behind the planned async DNS/transport boundary.

### LSP-001 language-server MVP is intentionally phase-limited (2026-07-29)

- Area: `crates/aura-lsp`, `auralsp` stdio server
- Symptom: the server currently supports lifecycle, full document sync,
  push/pull diagnostics, formatting, AST-backed symbols/completion, incremental
  edits, save/watch-folder refresh, workspace-folder changes, workspace-bounded
  navigation/refactoring, local and semantic member completion, source-comment
  hover documentation, versioned diagnostics, formatting, and compiler-suggestion
  code actions; overload-aware binding IDs and fully preemptive cancellation
  are not implemented. Diagnostics now resolve the complete package graph
  through the shared read-only loader, including standard, path, registry, and
  transitive dependencies, and rebuild it with open-document overlays. Hover
  type inference now reuses a diagnostics-warmed package cache keyed by
  manifest and analysis revision, while semantic member completion still
  relies on document-only analysis.
- Why deferred: the existing analysis API does not yet expose stable binding IDs
  or a structured suggestion model; package diagnostics still rebuild the
  resolved graph after document updates, lexical fallbacks remain conservative
  around unresolved local scope and overloads, and the stdio loop is serial.
- Next step: share the package cache with diagnostics, add package-aware member
  completion, precise binding IDs, and structured diagnostic suggestions before
  broadening rename/reference results, and move long queries behind a
  cancellable scheduler.

### ANALYSIS-001 analysis cache eviction is not implemented (2026-07-29)

- Area: `aura-analysis` snapshot query cache
- Symptom: parse and semantic results remain cached for every distinct document
  content seen by a host.
- Why deferred: the MVP needs the immutable snapshot/query contract first; LSP
  memory budgets and cache metrics need to be designed together.
- Next step: add bounded LRU/size-based eviction and expose cache hit/eviction
  metrics before enabling long-lived workspace sessions by default.

### ASYNC-002 generated payload clone integration (historical, superseded 2026-07-31)

> The bounded gaps described in this historical log are resolved by the
> current ASYNC-002 entry below. Remaining broader control-flow and aggregate
> limits are tracked under ASYNC-003.

- Area: compiler-generated async child-to-parent failure propagation
- Progress: native task outcomes now support clone-based terminal result
  propagation, preserving an independently owned successful payload and an
  explicit cancellation state. No-suspension generated async functions now catch primitive
  `Int`, `Bool`, and `String` exceptions, publish owned task error payloads
  through `aura_task_frame_set_error_span_with_clone`, and have native compile/
  run regression coverage. Compiler-generated `join` now returns
  `std.io.Result<T, std.io.TaskError>` and maps failed states to
  `TaskError.Failed(String)`, cancellation to `TaskError.Cancelled`, and
  unexpected pending states to a failed diagnostic; typed `spawn` now infers
  its return payload and native `TaskHandle<String>` success is covered by
  two repeated joins.
- Why still deferred: full `TaskError.Failed(error)` surface preservation and
  public raw typed outcomes and richer aggregate failures remain open. Generated class exceptions
  now retain an independently cloned raw payload alongside normalized owned
  error text; nested class payloads survive GC, parent propagation, and source
  release before two repeated typed joins. Lexical cleanup now automatically
  releases terminal `TaskHandle` bindings after their last scope, while
  pending handles now cancel and reclaim synchronously through handle release;
  executor ownership ends at the release boundary. Primitive `String` failure detail
  also survives a nested `leaf -> await middle -> spawned parent` chain and
  two repeated typed joins.
  The generic four-plus-await lowering now releases compiler-created child
  frames after terminal payload cloning and on parent frame destruction;
  caller-owned task handles are left untouched. Broader control-flow shapes
  still need the same explicit child ownership treatment.
  The
  single-await lowering now clones primitive `String` results into an owned
  parent result slot, and bounded `spawn` bodies can await String with repeated
  join observations; the bounded two/three-await and dynamic four-plus-await
  state machines now clone each primitive String suspension value into parent
  frame storage as well, and branch-join String payloads are cloned into owned
  result slots. The general four-plus-await state machine now also deep-clones
  `Array<Int>` suspension values into frame slots, permits forced GC between
  awaits, and deep-clones the final aggregate for repeated owning joins. Richer
  ownership, branch/loop CFG lowering and general state-machine paths remain
  open. The no-await primitive failure leak was
  fixed by allocating the result slot only after the body returns, avoiding a
  `longjmp`-orphaned allocation; this does not close the broader ownership
  contract. The runtime now also exposes `aura_task_outcome_clone` and
  `aura_task_owned_outcome_destroy`, proving an owned terminal snapshot can
  outlive its frame for success/error/cancel states. A local
  `Result<*, TaskError> = join(task)` now clones `Failed` detail and releases
  it at scope exit; local `Result<String, TaskError>` joins also clone
  successful strings through an owned `Ok` constructor and release them at
  scope exit. Bare joins remain borrowed, while successful non-String/richer
  payloads still need a corresponding generated ownership path. Native typed
  spawn coverage now includes suspended String, Int, Bool, and heap-class
  returns (including a child async single-await class result), repeated success joins, typed cancellation, and direct producer-side
  `Array<Int>` payloads with repeated owning joins; a branch-join `Array<Int>`
  result now deep-clones into the parent result slot, destroys nested owned
  contents exactly once, survives forced GC, and has native repeated-join and
  cancellation coverage. The general nested branch/loop CFG now also accepts
  an `Array<Int>` return and awaited slot, clones it at the await boundary,
  frees frame slots on cancellation, and proves two repeated owning joins.
  Rarer aggregate payloads still need matching poller ownership paths. A
  general nested `if -> while -> await` CFG now also clones suspended and
  returned `String` payloads into owned frame/result storage, with native
  success, repeated-join, failure-detail, queued-cancellation, and forced-GC
  coverage.
  Async frame GC marking now dispatches through generated `Array<T>` mark
  hooks, covering enum/struct elements and nested arrays across suspension; a
  native `Array<enum>` + await + forced-GC regression passes. The remaining
  open surface is the public raw typed failure payload at `join` (the join ABI
  exposes normalized `TaskError.Failed(String)` plus typed name and source-span
  metadata, while nested propagation retains the raw clone internally), plus
  richer unbounded CFG aggregate ownership.
- Progress: the general four-await state machine now accepts `Array<String>`
  suspension and return payloads, deep-clones each suspended array including
  owned String elements, survives forced GC between awaits, and passes repeated
  owning join plus queued-cancellation native coverage.
- Progress: the general nested `if -> while -> await` CFG now accepts any
  supported `Array<T>` payload, including deep cloning and destruction of owned
  `Array<String>` elements across success, failure, forced-GC, cancellation,
  and repeated owning joins.
- Progress: general CFG `for-in` awaits now iterate UTF-8 bytes from a `String`
  across suspension, with native repeated-join and queued-cancellation coverage.
- Progress: general CFG assignments now transfer `String` and array ownership
  when moving from one local to another and synchronize moved frame slots after
  each action. Native coverage exercises repeated awaits of caller-owned
  `Task<String>` and `Task<Array<String>>` through a loop, forced GC, failure,
  cancellation, and repeated owning joins under ASAN.
- Progress (2026-08-01): scalar awaits nested in binary/unary expressions and
  call/field evaluation are split into a typed frame local plus a post-resume
  continuation; native coverage executes `(await one()) + 1` through spawn and
  owning join.
- Progress (2026-08-01): expression lowering now recursively chains multiple
  awaits in one initializer or assignment. Each operand receives its own typed
  frame slot and failure/cancellation route, and the continuation evaluates
  only after prior awaits complete; native coverage executes
  `identity(await one()) + await two()` through a spawned task and owned join.
- Progress (2026-08-02): general CFG return statements can now suspend through
  an awaited expression, including branch-local `return await task()` and
  aggregate/String results. The compiler allocates a typed synthetic return
  slot, routes failure/cancellation through the normal await edge, and emits
  the terminal result from that slot; a native repeated-join fixture covers the
  branch path. Spawn bodies still require their own bounded state-machine shape
  when the await is directly inside `return`.
- Next step: connect remaining richer aggregate payload ownership and
  suspended await failure propagation to the clone/destroy boundary.

### ASYNC-003 legacy bounded-loop slices (superseded 2026-07-31)

- Area: compiler-generated async state-machine control flow
- Progress: the C backend now lowers the bounded post-await branch continuation
  shape in addition to the bounded shape
  `while (...) { if (cond) { val x: Int = await task } index = ... }` with
  the loop index and child handle stored in the frame. False branches skip
  task creation; pending true branches resume without repeating the
  iteration. A sequential multi-await loop body now uses one frame state and
  child slot per await, persists loop locals across every suspension, and
  continues the next iteration only after the final child completes. Native
  coverage now also forces GC between iterations and cancels a queued loop
  task before its first poll. `aura-codegen` has regressions covering both
  loop paths. A bounded branch-then-second-await shape now persists the
  selected branch, carries two child slots across three resume states, runs
  `gc_collect()` between awaits, and rejects cancellation before worker
  execution in a native regression. A loop CFG slice now supports multiple
  pre-await guard branches with `break`/`continue`, a distinct resumed child
  state, forced GC after completion, and queued-task cancellation in a native
  regression. A nested outer/inner Int loop slice now persists both loop
  counters and the accumulator across an inner await, resumes at the inner
  loop head, and forces GC after each child completion in a native regression.
  A branch join whose two arms assign awaited `Task<Int>` values to one local
  now persists the selected child through a shared resume state and emits a
  common post-join continuation, including GC calls, repeated joins, and
  cancellation coverage. Sequential multi-await loop frames now release
  compiler-created child frames after each terminal result and from frame
  destruction; caller-owned handles remain borrowed. Multi-conditional branch
  loops now apply the same cleanup for each selected temporary child. The
  common `if/else` single-await branch join now releases the selected temporary
  child after copying its result and on parent destruction. The one-armed
  `if` assignment continuation now releases its temporary child on terminal
  copy and parent destruction as well. The post-suspend call continuation now
  uses the same temporary-vs-caller-owned rule.
  The same bounded branch continuation now owns copied `String` payloads in
  the frame and result slot, with forced-GC and repeated-join native coverage.
  A bounded Int loop now supports an if/else await at each iteration, with one
  persisted child state, shared accumulator/index continuation, forced GC, and
  queued-task cancellation coverage.
  A bounded loop branch now also carries a frame-owned `Array<Int>` value across
  each await, clones the selected child result before replacing the previous
  value, destroys the frame value on cancellation, and has forced-GC,
  repeated-join, and queued-task cancellation native coverage.
  A bounded loop with two independent conditional awaits now persists separate
  child handles and resume states, skips unselected awaits without allocation,
  and runs its shared GC/accumulator continuation exactly once per iteration.
  Native coverage exercises all four condition combinations, repeated joins,
  and queued cancellation. The same lowering now supports three conditional
  awaits with four distinct resume states, forced GC, repeated joins, and
  queued cancellation coverage.
  The general nested `if -> while -> await` CFG shape now also supports
  primitive `String` locals and return values, including owned suspension
  cloning, failure propagation, repeated joins, forced GC, and queued
  cancellation in a native regression.
- General CFG handle locals now transfer ownership between locals and retain
  caller-owned `Task<ForeignHandle<T>>` results across repeated awaits. A native
  fixture covers forced GC, typed failure/cancellation, repeated joins, and
  `Result<ForeignHandle<T>, TaskError>` ABI generation.
- General CFG actions now synchronize frame slots after local ownership moves,
  preventing stale aggregate aliases after a completed await. Repeated
  caller-owned `Task<String>` and `Task<Array<String>>` awaits through a loop
  are covered with multiple resume states, forced GC, typed failure/cancel, and
  repeated joins.
- The same general CFG lowering now accepts `Bool` return values and
  `Task<Bool>` operands. A native branch/loop fixture exercises both selected
  paths, multiple resume states, forced GC between iterations, repeated joins,
  and queued cancellation.
- It now also accepts heap-class return values and `Task<Class>` operands. The
  native fixture proves class pointers survive branch/loop suspension, frame
  GC scanning, repeated owning joins, and queued cancellation; terminal class
  results use explicit GC root/remove-root cleanup.
- CFG dispatch now recognizes both `if -> while` and `while -> if` await
  nesting; unsupported statements still fail back to the existing specialized
  lowerings instead of being emitted through an unsafe partial path.
- A range `for (i in start..end)` body with one awaited `Task<Int>` now persists
  the cursor, endpoint, accumulator, and temporary child across suspension;
  forced GC, repeated typed joins, and queued cancellation are covered by a
  native codegen fixture.
- A general CFG `for (item in Array<Int>)` body with an awaited `Task<Int>` now
  persists the moved iterator, index, binding, and temporary child across
  suspension; forced GC, repeated typed joins, and queued cancellation are
  covered by native codegen fixtures. Nested array elements, await assignments,
  nested catch routing, and eight-await linear state machines are now covered
  by the general CFG path; richer aggregate ownership remains in the newer
  ASYNC-003 frontier below.
- This historical entry is retained only to preserve the evidence trail for
  the specialized lowering slices it superseded. Current residual ownership
  work is tracked once, below, instead of as a bounded-control-flow blocker.

### SAN-002 broader compiler-generated ownership remains out of scope (resolved mandatory gate, 2026-07-23)

- Area: compiler-generated ownership cleanup under sanitizer smoke
- Symptom: the mandatory sanitizer suite covers deterministic native seeds and
  all Aura-generated legs listed by `scripts/sanitizer-smoke.sh`; arbitrary
  compiler-generated programs outside that manifest are not exhaustive.
- Why deferred: exhaustive ownership proving for every possible generated
  control-flow/layout shape is a future compiler verification project, not a
  release-gate omission.
- Progress: task outcomes, GC roots, exception cleanup, I/O cleanup, FFI
  handles/callbacks/net, and HTTP async/hardening/health are now seeded and
  executed under fail-closed native LSAN coverage.
- Progress: exception payload deep-copy/destructor paths, Array/String
  temporaries, lambda boxes, and the complete current Aura sanitizer leg now
  pass with `detect_leaks=1`.

### SAN-003 ephemeral TCP bind is unavailable in this host sandbox (2026-07-28)

- Area: native sanitizer smoke environment
- Symptom: `scripts/sanitizer-smoke.sh --native-only` reaches
  `task_io_cleanup_sanitizer` but `aura_tcp_listener_bind(0, ...)` fails before
  the fixture can exercise its cleanup assertions.
- Why deferred: this is an environment-level socket capability failure, not a
  regression in the compiler-generated file round-trip slice; the same failure
  reproduces on an immediate retry.
- Next step: rerun the complete sanitizer gate on a supported host with
  ephemeral loopback binding, then remove this entry if the seed passes.

### RUNTIME-003 exception cleanup and bounded cause API (resolved 2026-07-26)

- Area: unchecked exception payload ABI
- Symptom: object payload cleanup now accepts an explicit destructor, invokes it
  exactly once on clear/leave, transfers it across rethrow, disposes an old
  payload before replacement, and runs it before an uncaught object exception
  aborts the process. Native ASAN/UBSAN/LSAN covers nested owned data, implicit
  leave, rethrow, replacement, uncaught cleanup, and scalar pending reset.
- Residual outside this contract: the cause API is intentionally bounded to the
  current exception boundary and source-span representation; broader async
  outcome propagation belongs to ASYNC-002/003.
- Progress: `runtime/tests/exception_payload_cleanup.c` remains in the
  sanitizer seed manifest; `corpus/control/exception_payload_cleanup.aura`
  provides the generated shallow-copy regression with a static field.
- Evidence: `runtime/tests/exception_payload_cleanup.c` and the native
  `exception_cause_api` codegen fixture cover cause ownership and lifecycle.

### H6 routing is synchronous and exact-match only (2026-07-22)

- Area: HTTP handler API
- Symptom: `aura_http_dispatch_routes` now supports exact method/path matching
  and deterministic 404/405/500 responses, but it does not suspend, spawn, or
  retain request/response buffers across an await.
- Why deferred: those lifetimes depend on H5 async integration and the A5/A6
  frame ownership contract.
- Progress: `runtime/tests/http_routes.c` covers route success, method mismatch,
  not-found, handler failure, and borrowed callback ownership.
- Next step: adapt the route table to async handler frames after H5 is complete.

### IO5 network backpressure remains deferred (2026-07-22)

- Area: async I/O/channel bridge
- Symptom: bounded channels now suspend/wake producers and consumers with FIFO
  ownership and deterministic close/cancel cleanup, but TCP/file completion is
  not connected to this bridge.
- Why deferred: IO3/IO6 still lack scheduler-integrated network operations and
  cross-platform async wake sources.
- Progress: `runtime/tests/task_channel.c` covers full/empty, ordering,
  cancellation, close, and payload destruction.
- Next step: connect readiness/completion events to the existing channel waiter
  contract after async I/O suspension is implemented.

### A7 remains a bounded task-outcome ABI (2026-07-22)

- Area: async exception outcomes
- Symptom: the runtime now exposes and tests success, failure, cancellation,
  bounded span offsets, and a cancellation-handler failure policy, but has no
  compiler file/line mapping or nested exception chains.
- Why deferred: those representations require typed async lowering and true
  exception unwinding; the C frame ABI must not imply either.
- Progress: `runtime/tests/task_outcomes.c` proves owned payload cleanup before
  join observation and deterministic cancellation cleanup under the existing
  executor. `runtime/tests/task_dependency.c` now verifies child failure
  propagation with an independent payload/source ID/span and distinct
  cancellation state; generated one/two-await edges emit the same mapping.
  Cancellation handlers are bounded to publishing a failure after cleanup and
  are covered by the same fixture.
- Next step: extend the typed outcome ABI after control-flow suspension defines
  source-span and nested-failure ownership.

### Unjoined task failures are bounded runtime diagnostics (S6, 2026-07-22)

- Area: task outcome policy
- Symptom: unjoined terminal failures now report through a borrowed hook or
  default stderr logger, but compiler-generated nested failure chains and
  process-level aggregation are not yet defined.
- Why deferred: those payloads depend on A6/A7 outcome representation and the
  release diagnostics contract.
- Progress: task/source identity, failure bytes, joined suppression, shutdown
  reporting, and cancellation exclusion are covered by the focused fixture.
- Next step: connect the hook to typed async outcomes and release telemetry.

### A8 sanitizer fixture is bounded to the current frame ABI (2026-07-22)

- Area: runtime async ABI sanitizer coverage
- Symptom: the new sanitizer fixture proves root retention and cleanup for
  pending, cancellation, repeated polling, dropped-handle, and failure paths,
  but cannot exercise compiler-generated live-local hoisting or a delayed
  asynchronous wakeup.
- Why deferred: A4–A7 still define only bounded state metadata and the
  single-threaded executor contract; full suspension lowering and wake sources
  are not present in the runtime API.
- Progress: `runtime/tests/task_frame_sanitizer.c` runs the supported paths
  under ASAN/UBSAN, with an LSAN attempt when the host supports leak detection.
- Next step: extend the sanitizer matrix after typed frame roots, resume edges,
  and async wakeup semantics are implemented.

### S5 cancellation boundaries remain bounded (2026-07-22)

- Area: spawn cancellation and executor outcomes
- Symptom: request/acknowledgement ordering and joined/unjoined cleanup are
  defined for the current single-threaded ready/pending frame API, but await,
  I/O, handler, and concurrent completion boundaries are not implemented.
- Why deferred: those boundaries require the async suspension state machine and
  external wake sources; the bounded executor has one deterministic scheduler
  linearization point.
- Progress: `runtime/tests/task_cancellation.c` proves request acceptance,
  cancellation acknowledgement after cleanup, completion-wins ordering, and
  identical joined/unjoined cancellation outcomes under ASAN/UBSAN.
  `runtime/tests/task_dependency.c` now proves cancellation detaches a
  parent waiting on a child without cancelling the child implicitly.
- Next step: extend cancellation checks to file/network operations and handler
  frames once their wake/capture ownership exists.

### File I/O has no scheduler suspension yet (IO2, 2026-07-22)

- Area: runtime file operations
- Symptom: `AuraFile` provides bounded, status-based POSIX descriptor calls,
  but regular-file operations can still block in the host kernel and are not
  registered with an Aura async executor.
- Why deferred: the runtime has no completed A4–A8 suspension state machine,
  cancellation wakeup, or GC frame-root contract to safely park a file
  operation.
- Progress: open/read/write/flush/close/destroy own descriptors explicitly,
  borrow buffers only per call, classify permission/pending/EOF/closed/error,
  and are covered by `runtime/tests/file_io.c` under ASAN/UBSAN. The bounded
  task cleanup fixture additionally proves file/TCP resources release exactly
  once on cancellation, failure, forced shutdown, and peer disconnect.
- Progress: `runtime/tests/task_fd_wait.c` now keeps a GC-rooted capture with
  an `AuraFile` handle and read buffer alive across a pending descriptor wait,
  an intervening GC collection, resumption, and terminal release under
  ASAN/UBSAN.
- Next step: integrate regular-file operations with a true asynchronous
  operation backend once a portable completion source exists; the bounded IO2
  preservation item is complete, but regular-file scheduler suspension is not
  claimed.

### IO4 native operation adapters remain open (2026-07-23)

- Area: async file/TCP cancellation and cleanup
- Symptom: `AuraTaskFrame` now provides an exactly-once cleanup hook for a
  registered pending resource, and the bounded native peer-disconnect path
  cleans file/TCP resources. TCP listener/stream descriptors now have bounded
  frame wait adapters, including a descriptor-backed `AuraFile` adapter, but
  `AuraFile` operations and full TCP operation ownership still do not expose a
  complete async operation handle.
- Why deferred: the current POSIX APIs perform bounded synchronous calls (or
  return `PENDING` for a zero-timeout probe), so claiming disconnect cleanup or
  scheduler-wide failure wakeup would overstate the ABI.
- Progress: `runtime/tests/task_io_cleanup_sanitizer.c` proves file and TCP
  descriptor cleanup on cancellation, failure, forced executor shutdown, and
  peer disconnect under ASAN/UBSAN. The bounded frame ABI now exposes an
  adapter-owned waiting token plus `aura_task_executor_wake_waiting`, which
  clears and queues a waiting frame exactly once. `runtime/tests/task_fd_wait.c`
  additionally proves inline POSIX fd registration, timeout, multi-descriptor
  readiness wake, cancellation cleanup, and real file/listener/stream adapter
  waits under ASAN/UBSAN.
- Next step: define an operation handle and full readiness/event completion
  after the A4–A8 suspension contract is available; add file semantics and
  connect real disconnect/failure completion before closing the remaining IO4
  items.

### HTTP H3 remains transport-independent (2026-07-22)

- Area: runtime HTTP response builder
- Exclusion: H3 does not write sockets, run request/response loops, derive
  keep-alive from parsed requests, emit server diagnostics, or provide chunked
  framing, compression, HTTP/2, or TLS.
- Progress: bounded owned response state, strict header/status validation,
  deterministic HTTP/1.1 serialization, and stable 400/405/413/500 JSON error
  bodies are implemented and covered by `runtime/tests/http_response.c`.
- Next step: H4+ must define lifecycle and diagnostic integration before this
  builder is used by a server; the builder defaults to `Connection: close`.

### HTTP H4 remains synchronous and callback-only (2026-07-22)

- Area: runtime HTTP connection lifecycle
- Symptom: H4 provides a bounded blocking request/response loop and an opaque
  callback, but does not suspend tasks, route methods/paths, or serve multiple
  connections concurrently.
- Why deferred: async suspension and ownership across awaits belong to H5;
  handler/routing API belongs to H6. The callback intentionally avoids making
  either contract implicit.
- Progress: TCP partial reads/writes, idle/read/write timeouts, peer close,
  keep-alive, request limits, active-connection limits, and graceful listener
  shutdown are covered by `runtime/tests/http_connection.c`.
- Next step: implement H5 async integration and H6 handler/routing only after
  their dependency contracts are frozen.

### HTTP hardening remains bounded native coverage (H8, 2026-07-22)

- Area: HTTP hostile-input and lifecycle acceptance
- Symptom: the hardening fixture covers oversized input, malformed framing,
  partial-client timeout, connection limits, and forced shutdown, but does not
  provide fuzzing, async suspension, cross-host results, or routing coverage.
- Why deferred: those behaviors belong to the fuzz/release matrix, H5 async
  integration, and H6 handler/routing API respectively; H4 is intentionally
  synchronous.
- Progress: `runtime/tests/http_hardening.c` runs the bounded checks over the
  existing parser and connection APIs under strict ASAN/UBSAN (and LSAN when
  supported). The hardening fixture and `examples/http-health` native
  companion now run from `scripts/sanitizer-smoke.sh`; the companion README
  records the Linux result and the unverified macOS host.
- Next step: keep the remaining H6 routing and cross-host acceptance open,
  then add supported-host evidence once the documented Aura-level server path
  exists.

### HTTP async handler and Aura typed-handle gaps remain (H5, updated 2026-07-31)

- Area: async HTTP connection integration
- Symptom: the supported HTTP/1.1 handler path is complete on the covered
  native targets; extended protocol coverage and cross-target evidence remain
  outside the current acceptance matrix.
- Why deferred: TLS/HTTP2/HTTP3/WebSocket/multipart support and additional
  hosts require separate protocol and platform contracts; a fallback would
  weaken cancellation and backpressure guarantees.
- Progress: `runtime/tests/http_async.c` now proves a task handler can retain
  request/response state across a readiness suspension and cancellation, in
  addition to independent pending
  connections, cancellation, peer disconnect before a complete request,
  pipelined keep-alive, a later keep-alive request after suspension, and
  bounded POLLOUT response backpressure; all `runtime/tests/http*.c` fixtures
  and both HTTP health smoke paths run directly under ASAN/UBSAN. The full
  sanitizer manifest now includes this fixture under native ASAN/UBSAN.
- Progress: `aura_http_connection_poll_async_task_handle` now pins a typed
  `AuraFfiOpaqueHandle` across request-readiness and handler suspension, and
  releases the pin only after terminal response/cancellation cleanup. The HTTP
  async fixture covers dropping the lexical handle owner while the task is
  pending; the connection remains live through the pin.
- Progress (2026-08-01): the handle-backed poll path arms its connection
  cleanup hook immediately after acquiring the task pin, so an early buffer
  allocation or initialization failure cannot strand the pin before normal
  async initialization installs cleanup.
- Resolved (2026-08-01): general handler CFG lowering now covers branch/loop
  joins, repeated awaits, typed failure, nested cancellation/finally, async
  class methods, and scheduler-owned locals. Native regressions cover nested
  cancellation cleanup, `TaskHandle` and `Channel` locals across suspension,
  plus the full sanitizer matrix. Unsupported spawn-body shapes remain an
  explicit compiler rejection, not a runtime fallback.
- Progress: runtime typed borrowed accessors now expose request method, target,
  version, bounded headers/body, and response status/headers/body/keep-alive.
  Strict parser/response fixtures verify bounds, binary body bytes, and that
  accessors do not create a second owner.
- Progress: compiler-generated `std.http` accessors now pin borrowed
  `ForeignHandle<Int>` resources at the sync boundary, copy request text/body
  values into owned strings, and expose bounded response read/mutation calls;
  a compiler ABI/link fixture covers the ownership boundary.
- Progress: `std.http` now declares `Request` and `Response` wrappers around
  those package-private bridge functions, includes owned header accessors, and
  installed-stdlib materialization includes the package. Their bridge fields
  are private; the constructor handle parameter remains a temporary visibility
  limitation that applications must not treat as a supported raw-handle API.
- Progress: `std.net.listen` now creates an owned `AuraTcpListener` handle and
  `std.net.accept` lowers to a task that pins the listener, waits on `POLLIN`,
  returns an owned stream handle, and unpins or destroys exactly once on every
  terminal path. `closeListener` and `closeStream` also expose the runtime's
  idempotent terminal transition through a checked synchronous pin. The
  compiler fixture proves the generated ABI; an Aura HTTP server still needs
  the handler-to-`AuraHttpTaskHandler` lowering.
- Progress: a failed `AuraHttpTaskHandler` now replaces its partial builder
  with a bounded `500 handler_failure` response and closes only after that
  response is written. `runtime/tests/http_async.c` contains the regression;
  running it requires a host that permits ephemeral loopback sockets.
- Progress (2026-07-29): `std.http.serveConnection` now lowers an accepted
  `std.net` stream into a connection-owned runtime task, builds typed
  `Request`/`Response` wrappers, waits on the handler's child task, and tears
  down roots, bridge handles, closure environments, and the connection on all
  terminal paths. `std.http.serve` owns the listener readiness loop and starts
  one connection task per accepted stream; `examples/http-health-aura` builds
  against that API with a handler that awaits before responding.
- Progress (2026-07-29): the Aura-level server now retains and reaps terminal
  connection frames and terminal handler child frames. It admits at most 64
  active connections, relying on the listener backlog while full; existing
  native request/response byte limits and readiness-based partial writes
  provide the remaining bounded-memory/backpressure edge.
- Progress (2026-07-29): closing the `std.net` listener now wakes a waiting
  `std.http.serve` task and maps `AURA_TCP_CLOSED` to normal completion. The
  `corpus/std_http/shutdown` fixture builds and exits, covering the Aura-level
  graceful-shutdown signal; streaming bodies and external signal integration
  remain deferred.
- Progress (2026-07-29): executor liveness now ignores terminal frames held
  temporarily for borrowed join results, while the HTTP server uses the
  dedicated terminal-reap path to unlink and destroy directly-polled child
  frames. This preserves result-borrowing semantics during main-loop drain.
- Progress (2026-07-30): the shared executor now rejects submissions after a
  configurable live-task cap (default 4096, hard cap 65536) and admits new work
  after terminal frames are released. Native HTTP async coverage proves
  rejection/recovery alongside the server's active-connection limit.
- Progress (2026-07-29): readiness waits now use monotonic deadlines. Async
  HTTP read/idle timeout wakes the frame, serializes one bounded `408
request_timeout` response, then closes; runtime timeout and sanitizer
  regressions cover the scheduler and HTTP path.
- Resolved (2026-08-02): the parser now accepts bounded inbound HTTP/1.1
  chunked bodies, decodes them into the request-owned snapshot, preserves the
  next keep-alive request boundary, and retains validated trailer fields.
  Streaming readers validate and consume trailer fields before EOF while
  rejecting framing fields. `runtime/tests/http_parser.c` covers both full
  snapshots and streaming readers under the sanitizer manifest; the
  `examples/http-health-aura` route streams a chunked body after two handler
  awaits. Streaming task readers/writers use a distinct cross-await ownership
  contract.
- Progress (2026-07-29): an internal header-first parser now validates and
  owns request metadata without waiting for the body, preserving the exact
  header boundary and Content-Length/chunked framing. Native parser coverage
  proves partial-body input is not copied or consumed. The async task-handler
  connection path uses it for non-empty Content-Length and chunked requests;
  synchronous handlers intentionally retain snapshot parsing.
- Progress (2026-07-29): a Content-Length reader core consumes bounded chunks
  from that unread buffer or directly from the socket without reading beyond
  the declared body, preserving pipelined bytes. Strict native regressions
  cover both paths. A task-handler regression proves partial-body suspension,
  EOF, and the next pipelined request; early handler completion forces close.
  `Request.bodyReader().readChunk(capacity)` now lowers an Aura async class
  method to the same bounded reader task; corpus and `/stream` smoke evidence
  cover the public call. The reader releases its exclusive claim on terminal
  read so sequential awaits do not depend on executor frame reclamation.
  `Response.writeChunk` now has equivalent runtime-backed lowering with
  chunked framing and partial-write waits; `/stream-response` smoke covers two
  chunks. Async class methods now route through the bounded async lowering
  dispatch, retain a synthetic `this` slot, and rebind it in the covered CFG
  resume path; less-common specialized await shapes and arbitrary class-method
  CFGs still need the general method-aware state-machine path. Chunked request
  trailers are now validated and retained in full snapshots; streaming readers
  consume them before EOF while keeping them internal to the body-reader API.
  `Request.body()` remains a snapshot accessor for paths that materialize one;
  header-first task handlers must use `bodyReader()` for non-empty bodies.
- Progress (2026-07-31): compiler coverage now includes an HTTP handler whose
  async child uses the general CFG lowering across multiple awaits and a
  branch; the connection bridge continues to propagate child failure and
  cancellation through the existing waiter contract.
- Progress (2026-08-01): generated `RequestBody.readChunk` and
  `Response.writeChunk` frame destructors now have verified cleanup for
  buffers, read claims, and FFI pins across cancellation/failure paths; the
  codegen suite covers both streaming methods and the server cancellation
  bridge.
- Progress (2026-08-01): handle-backed HTTP polling now detects an already
  armed frame cleanup callback instead of executing it a second time during
  initialization. Terminal handle pins are released by frame cleanup after
  polling returns, preventing a re-entrant connection destroy/reset loop.
  The standalone `runtime/tests/http_async.c` fixture and the full sanitizer
  gate pass on the native host.
- Progress (2026-08-01): the general async CFG now lowers `if (await BoolTask)`
  and `while (await BoolTask)` in async class methods. The awaited condition is
  stored in a typed frame slot, participates in cancellation/failure cleanup,
  and resumes into the branch/loop edge without re-evaluating the task. Native
  method-aware branch and loop fixtures pass; arbitrary expression-level await
  composition and cross-target evidence remain open.
- Progress (2026-08-02): expression-form `if (await condition())` now splits
  the awaited condition into the same typed frame slot and resumes through the
  value continuation. A native `Task<Int>` join fixture covers the handler-safe
  expression path.
- Progress (2026-08-02): expression-form `if` branches containing awaited
  values now lower to explicit branch states and typed target assignments. Both
  selected and unselected branches are covered by a native `Task<Int>` fixture;
- Progress (2026-08-02): the compiler HTTP handler fixture now exercises nested
  branch-awaits inside a loop and inside both the protected and catch arms of a
  try/catch. The generated handler still uses the same cancellation bridge and
  typed frame state machine, so this combination is compile-checked rather than
  treated as a handler-specific synchronous shortcut.
  nested expression-if composition remains subject to the same general CFG
  operand-shape checks.
- Progress (2026-08-02): nested awaits inside async operation operands (for
  example `await await makeTask()` and awaited task/channel handles) now lift
  the innermost await into the general CFG continuation instead of being
  re-evaluated or rejected as an unsupported expression shape. A native nested
  await regression covers the resumed value and repeated outer join.
- Progress (2026-08-02): compiler-generated `RequestBody.readChunk` now enters
  the runtime's exclusive body-reader claim after pinning and holds it across
  pending readiness. Success, failure, cancellation, allocation failure, and
  frame destruction all release the claim before unpinning, so escaped or
  overlapping readers cannot advance one connection-owned parser concurrently.
- Resolved: general async lowering now handles the supported suspending handler
  control-flow contract, including branch/loop joins, repeated awaits, typed
  failure, cancellation/finally, async class methods, and scheduler-owned
  locals. The Aura example's loopback curl smoke and the sanitizer matrix pass;
  cross-target evidence and extended protocols remain separate acceptance work.
- Progress (2026-08-01): additive `std.net` typed wrappers now cover listener
  bind, stream connect, and idempotent close as
  `Outcome<..., NetError>`, while the legacy Bool/raw-handle functions remain
  documented compatibility shims. `std.http.getResponseResult` and
  `postResponseResult` now classify transport exceptions as `HttpError` as
  well as malformed-response framing; corpus builds cover the generated
  aggregate Outcome/ForeignHandle ABI.
- Remaining: the legacy `std.net`/`std.http` entry points still exist for alpha
  compatibility, and full RFC-007 naming (`Result<T, PackageError>`) awaits
  the merged-package generic enum resolver and S01/S06 error migration.
- Progress (2026-07-29): `std.http.get` now provides a bounded loopback
  HTTP/1.1 GET path over `std.net`; `corpus/std_http/client` and
  `scripts/http-aura-smoke.sh` compile it as a standalone `std.http` consumer
  and verify a real Aura server's `200 OK` response. It uses bounded
  `std.net.readAllStream` to continue through EOF rather than assuming one TCP
  read contains a full response. The compiler emits the server-handler
  fat-pointer ABI even when the app itself has no handler lambda.
- Progress (2026-07-29): `std.http.post` and `postResponse` add the same
  bounded loopback transport for one request body. They generate an exact
  `Content-Length`, close after the response, and `corpus/std_http/client_post`
  verifies the Aura `/echo` path in `scripts/http-aura-smoke.sh`.
- Remaining: this helper returns unparsed raw response bytes, only targets
  loopback, and has no custom request headers, timeout result, pooling,
  streaming, TLS, HTTP/2, or HTTP/3 support. `getResponse` and `postResponse`
  now supply a bounded status/body view but map malformed framing to status
  zero until the typed error model exists. Generic `spawn` capture lowering
  still needs initializer inference for richer expressions and open generic
  local types.
- Progress (2026-08-01): bounded spawn capture now accepts `Opt_Int` and
  `Opt_Bool` locals by value; a native capture regression verifies an inferred
  optional local remains available in the spawned frame.
- Progress (2026-08-01): unannotated locals whose initializer is a generic
  aggregate expression now reuse semantic expression types during static frame
  discovery; `identity(Array<Box>(1))` is captured by value with the generated
  Array clone/root contract and runs natively.
- Progress (2026-08-01): owning `join` now copies `Opt_Int` and `Opt_Bool`
  task results as scalar tagged values, with repeated-join/forced-GC native
  coverage; optional payloads do not enter aggregate cleanup.
- Next step: extend typed client outcomes with timeout/cancellation-specific
  classification and add runtime execution coverage for failed connect and
  malformed-response paths when loopback socket tests are available.

### Async suspension GC roots and ownership (C22s, 2026-07-22)

- Area: async/task runtime and codegen
- Symptom: the C22 task frame owns opaque `data` bytes but has no GC mark hook; captured heap-class references cannot safely survive a future `await` suspension. C22o channel payloads are currently safe only because class values use a temporary GC-rooted box and `Int`/`String` values transfer their malloc ownership to the receiver.
- Why deferred: the complete C22l state-machine/capture lowering and frame-root
  contract are not implemented; the shipped slice is limited to bounded
  straight-line async bodies.
- Progress: frame captures, pending operations, results, and errors now have explicit ownership metadata, GC root registration, borrowed-value rejection, and exactly-once release. The compiler already rejects borrowed values crossing await/spawn/channel boundaries. The runtime exposes paired typed frame-data mark/drop hooks; the general CFG and the specialized loop/branch Array lowerer now register generated callbacks, with aggregate cleanup in the exactly-once drop callback and native regression coverage.
- Progress: value-struct aggregates now use the same generated clone/drop/mark hooks in the general branch/loop CFG, including parameter capture, await transfer, result destruction, repeated owning joins, and forced-GC native coverage.
- Progress (2026-08-01): bounded `spawn` frames now register cloned
  `Array<HeapClass>` captures as GC array roots and unregister them before
  releasing the buffer. A scoped-capture regression drops the outer array,
  forces GC across an await, and still reads the class element successfully.
- Progress (2026-08-01): async CFG caught-class frames now mark aggregate
  fields through their typed Array/enum/struct/class hooks. Exception lowering
  also unregisters lexical Array roots before `longjmp`, preventing stale stack
  slots from entering the runtime root table; forced-GC class-array catch
  coverage passes under the native sanitizer build.
- Progress (2026-08-01): the bounded first-await state machine now stores and
  republishes `Opt_Int`/`Opt_Bool` values without treating their tagged scalar
  storage as an aggregate root; native repeated-join coverage exercises the
  suspended path.
- Progress (2026-08-01): discarded `Unit` awaits in bounded spawned tasks now
  use a static frame task slot and propagate child failure/cancellation through
  the same cleanup contract as value awaits; nested typed class failure and
  native sanitizer coverage pass.
- Progress (2026-08-01): the specialized loop/branch `Array<T>` lowerer now
  uses the generated recursive Array mark hook and checked enum/struct drop
  helper. A repeated-join forced-GC fixture with `Array<enum>` payloads passes,
  closing the prior specialized-path gap for those aggregate elements.
- Next step: apply the callback contract to the remaining specialized async lowerers and richer nested aggregate layouts; the conservative frame scan remains the compatibility fallback until every generated layout has that metadata.

### Async lowering and task outcome gaps (C22t, historical, superseded 2026-08-01)

- Area: async/task codegen and runtime outcomes
- Superseded: general CFG lowering now covers branches, loops, richer aggregate
  values, typed failure/cancellation propagation, and repeated owning joins;
  the remaining narrower ownership cases are tracked by ASYNC-002, C22s, and
  ERROR-002 below.
- Progress: the compiler emits explicit entry/resume states for one through
  three awaits, hoists live Int/String locals into frame data, and uses a runtime
  parent-child waiter list to wake the parent exactly once.
  runtime/tests/task_dependency.c covers delayed child completion under
  ASAN/UBSAN.
- Next step: keep the specialized lowerers aligned with the generated frame
  mark/drop callback contract as their remaining aggregate cases are migrated.

### S4 source locations and nested failures remain bounded (2026-07-22)

- Area: runtime task failure outcomes
- Symptom: the bounded executor retains a numeric source identity with each
  failure, but does not yet carry file/line/column metadata or nested exception
  chains through compiler-generated async frames.
- Why deferred: those fields depend on the A4–A7 suspension/state-machine and
  diagnostic payload contracts; inventing a second source-location format in
  the C-only runtime would not prove end-to-end propagation.
- Progress: failed joins preserve payload and source ID across repeated
  observation; result/error slots clear before GC-root removal and user cleanup,
  and terminal release is covered by `runtime/tests/task_join.c`.
- Next step: extend the typed compiler outcome ABI when async lowering defines
  source spans and nested failure representation.

### S3 release rehearsal external blockers

- Area: production release / S3.2 + S3.6
- Symptom: this offline rehearsal can exercise only the current host's native
  target. macOS amd64/arm64 and Linux amd64 each still need a matching clean
  host run; a cross-compiled archive is not treated as a runtime pass.
- Blocker: published installer smoke requires the release URL, CDN availability,
  GitHub release assets, and credentials/permissions outside this repository.
- Next step: on each supported clean host, run `bash scripts/install-smoke.sh
--from-release` against the frozen release, then record the URL, target, and
  checksum result. Keep failed/interrupted-install evidence with the release
  ticket; the offline script only proves failed archive verification preserves
  the active `current` link.

### C22 release work deferred (C22t, 2026-07-22)

- Area: release / publication
- Symptom: C22t records implementation status only; no new release rehearsal, signing, publication, or cross-target artifact work was performed.
- Why deferred: release execution is outside the C22 scope and requires an explicit release request plus external hosts, credentials, and distribution services.
- Next step: create a separate release task after await/capture/failure gaps are resolved and run the supported-target acceptance matrix.

### Lambda capture limits (MVP)

- Area: language / lambdas (C10h/C12k/C12l/C12m/C13e/C13f/C13g)
- Symptom: richer aggregate captures and concurrent scheduler policy remain bounded; mutable Array capture itself is no longer a borrowed-view MVP (immutable Array captures own a cloned snapshot)
- Why deferred: richer aggregate element types and scheduler policy need separate contracts; the Array capture ownership contract is now explicit and implemented
- Progress: C20c–e add shared pointer boxes and codegen lowering for mutable class/Array/Fun captures; class payloads are GC-rooted, nested Fun environments retain/release, and corpus covers mutation, rebinding, escaping closures, and GC churn. Mutable Array captures use one owned `aura_box_ptr` shared by the outer binding and every closure; native fixtures verify mutation after outer-scope escape and shared visibility across two closures. Existing env `__drop` still unregisters class roots / releases boxes / nested Fun envs then frees (never frees Array buffers)
- Progress (2026-08-01): immutable `Array<HeapClass>` captures in bounded
  spawned tasks now register the cloned element buffer as an explicit GC array
  root for the full task lifetime; a forced-GC/await regression covers the
  scope-escape case.
- Progress (2026-08-01): escaping immutable lambda captures now apply the same
  array-root contract for cloned `Array<HeapClass>` snapshots; env drop removes
  the root before recursive element cleanup. Native closure-escape/forced-GC
  coverage reads a captured class element after the outer scope ends.
- Progress (2026-08-01): immutable `Array<Interface>` captures now use a typed
  array-root callback that scans each tagged union element through its generated
  interface mark hook. The cloned snapshot remains valid after owner scope exit,
  mutation, and forced GC; native ASan coverage dispatches through a heap-class
  implementor after closure escape.
- Progress (2026-08-01): the typed root registration is now shared by compiler
  generated locals, parameters, async frames, caught aggregate copies, spawn
  captures, and mutable capture boxes. Nested arrays delegate to recursive mark
  hooks; native coverage also exercises an escaping `Array<Array<Interface>>`
  snapshot after forced GC.
- Progress (2026-08-01): lambdas nested directly in async function bodies are
  now included in the static lambda emitter; function-valued async results
  retain/release their closure environment across suspension and terminal
  cleanup, covered by a native `Task<(Int) -> Int>` regression.
- Note (C12l): immutable Array capture clones `{data,len,cap}` into the closure environment; mutable `var` Array capture uses an owned shared cell, not a borrowed view, so owner movement is cell retain/release and no mutation invalidation is exposed
- Note (C12m/C13f): `var` Int/Bool/String uses `aura_box_*` (refcount); String box owns heap copy (`set` frees previous); outer + each capturing env retain; multiple lambdas share mutations; escaping Fun keeps the box alive
- Note (C13g): Fun param transfer moves env (caller must not call after pass); nested retain via capture keeps both live — stress corpus documents both
- Next step: specify richer aggregate element captures and scheduler policy without changing the shared-cell Array contract
- Note: C12 batch closed (C12t); C13e Fun + C13f var String + C13g stress audit shipped; C20c–e mutable class/Array/Fun shared ownership contract shipped — residual is richer aggregate/scheduler policy
- Introduced: narrowed after C10h; env free 2026-07-20; class C12k 2026-07-21; Array view C12l 2026-07-21; var Int/Bool C12m 2026-07-21; Fun C13e 2026-07-21; var String C13f 2026-07-21; stress C13g 2026-07-21; mutable class/Array/Fun MVP C20c–e 2026-07-22

### Array field ownership contract (resolved 2026-08-01)

- Area: builtin Array (C7c/C8j)
- Resolution: Array field reads are lexical borrows. Assigning a field to
  `ref Array<T>` is allowed only while the receiver is live; returning,
  storing, capturing, awaiting, spawning, or channeling that view is rejected.
  `Array.clone()` is the explicit owning escape hatch.
- Evidence: sema coverage proves valid scoped field views, rejects
  `return this.items`, rejects closure escape, and native lambda/GC fixtures
  prove owned immutable snapshots and shared mutable cells do not use a stale
  non-owning view.
- Introduced: narrowed after C8j; clone C9c; lexical borrow enforcement C22i.

### Registry publishing and alternate dependency sources

- Area: toolchain / RFC-005
- Symptom: `aura publish`, registry authentication, and `github=`/`git=` dependency sources are not implemented.
- Why deferred: the S2 release contract covers consuming locked packages; hosting,
  accounts, and publishing require an external registry API decision.
- Progress: lock schema, semver pinning, SHA-256 verification, HTTPS metadata and
  archive downloads, nested registry resolution, atomic cache publication, and
  production acceptance coverage are complete.
- Next step: define the registry API/authentication contract before implementing
  `aura publish` or alternate dependency sources.
- Introduced: narrowed after C3p; HTTPS/nested registry work completed in S2

### Publish signing and dependency resolution in dry-run (U4, 2026-07-22)

- Area: package publication preview
- Symptom: `aura publish --dry-run` previews an unsigned archive and validates
  registry dependencies only from an existing local lock pin; it does not sign,
  resolve, fetch, or upload.
- Why deferred: signing keys/policy and the U5 registry upload protocol are not
  yet defined; keeping the preview read-only prevents false release claims.
- Next step: add a signing primitive/key policy and U5 upload orchestration,
  then extend the preview to verify the exact publish metadata end to end.

## Resolved

### Stdlib live collection iterators (C20j, 2026-07-29)

- Resolved: `HashMap.liveKeyIterator()` and `liveEntryIterator()` plus
  `HashSet.liveIterator()` retain their source, traverse logical table order,
  and invalidate on structural epoch changes. Invalid cursors are terminal;
  value replacement remains visible. `HashMapLiveEntry` results are checked
  views rather than raw bucket aliases.
- Evidence: `corpus/std_collections/live_iterator` covers value replacement,
  map insertion invalidation, set removal invalidation, and terminal `next()`.

### Array of interface elements (C20h, 2026-08-01)

- Resolved: `Array<I>` uses the runtime tagged-interface representation with
  element copy/drop helpers, interface dispatch, and typed GC marking. Lambda
  snapshots register their backing buffers with the typed root ABI, so the
  collector scans tags before marking heap implementors. Native fixtures cover
  storage, dispatch, collection after GC, and escaping immutable captures.
- Historical design alternatives remain documented in
  `docs/plans/2026-07-22-c20h-array-interface-spike.md`; they are not the
  shipped ABI.

### Generic class construction inside generic bodies (C19x, 2026-07-22)

- Resolved: codegen now substitutes generic function/method type arguments
  before emitting generic class constructor symbols, including alias-qualified
  constructors. Corpus `generic/constructor_subst.aura` covers both a generic
  function and a generic-class method returning concrete `Pair` monomorphs.

### C16 generic HOF compiler support (2026-07-21)

- Resolved: sema accepts generic function parameters such as `(T) -> R`; codegen skips open generic `Fun<T, R>` typedefs and emits only concrete monomorphs, allowing generic `map`/`filter`/`fold` implementations to compile.

### C16 generic HOF stdlib coverage (2026-07-21)

- Resolved: `std.collections` generic `map<T,R>`, `filter<T>`, and `fold<T,A>` are exercised end-to-end by corpus packages for `Array<Int>` and `Array<String>`.
- Extended by C17: generic HOF codegen is exercised end-to-end with `Box<Int>` as both element and accumulator.

### C17 generic HOF user-defined class coverage (2026-07-21)

- Resolved: generic `map<T,R>`, `filter<T>`, and `fold<T,A>` compile and run with a generic heap class `Box<Int>` in `Array<Box<Int>>`.
- Coverage remains focused on closed monomorphs; interface elements and richer nested generic layouts remain separate work.

### C14 generic HashMap (2026-07-21)

- Resolved: compiler-backed `Hashable` for `Int`/`String`, generic open-addressing `HashMap<K,V>`, compatibility factories, Int-key corpus, and collection docs.

### C15 generic HashSet (2026-07-21)

- Resolved: generic open-addressing `HashSet<T : Hashable>` backed by `HashMap<T, Bool>`, String factory, Int-key corpus, iteration API, and collection docs.

### C18 generic hash-collection HOFs (2026-07-21)

- Resolved: `HashMap.keyArray`/`valueArray`, `HashSet.toArray`, and generic free functions `map_hash_map_values`, `filter_hash_set`, and `map_hash_set`; Int and String runtime corpus coverage added.
- Limitation: Aura methods cannot declare their own type parameters (C2b), so HOFs use explicit free-function names and return arrays rather than entry tuples or new collections.

### C13 batch (2026-07-21)

- Resolved C13a–t: method-on-temp; `Int.toString` + String↔Int `+`; Array\<String\> elem free; Fun + `var` String captures + stress; capture reject diags; registry K1 offline (index/semver/fetch/build); `toLower`/`toUpper`; eprint corpus; `tryWriteFile`; Hashable spike; `examples/wc` polish; signing design note; docs close.
- Residual: registry publishing/authentication; stdlib generic HOF API; true borrow;
  `var` class/Array/Fun.

### Process argv string ownership (`Io.args`) — S1.1

- Resolved: `aura_args_get` now returns a heap-allocated copy for each process argument, matching `Array<String>` element destruction.
- Regression: `aura-cli` builds and executes `corpus/std_io/args` with forwarded arguments and verifies successful teardown.
- Resolved: 2026-07-21

### Chained method on `Array.get` temporary (codegen) — C13b / C13q

- Resolved: method-on-temp for call-result receivers; `examples/wc` uses `segs.get(j).trim()` and `argv.get(i).trim().toInt()` without intermediate binds.

### No std Int→String (CLI print) — C13c / C13q

- Resolved: builtin `Int.toString()` (+ String/Int `+`); `examples/wc` prints counts with `.toString()` (local `u64ToString` removed).

### Array element drop for String (C13d)

- Resolved: free owned `const char *` elems on Array\<String\> drop/clear/set; push/set heap-copy. Residual: process argv arrays (see open debt).

### C12 post-alpha batch (2026-07-21)

- Resolved C12a–t: process argv/stdin/exit; String `indexOf`/`split`/`trim*`/`toInt`; `join`; lambda class/Array/`var` Int·Bool captures; HashMapStr; String HOF; `tryReadFile`; `examples/wc`; guide/corpus/install smoke; batch close. Residual open debts (Fun capture, generic HashMap, String free, method-on-temp, Int→String, registry, borrow, Array&lt;I&gt;) unchanged in scope.

### Higher-order Int array helpers (2026-07-20)

- Resolved in C10i: `std.collections` `map_ints` / `filter_ints` / `fold_ints`; corpus `fun/lambda_hof.aura`, `std_collections/hof`.

### Higher-order String array helpers (2026-07-21)

- Resolved in C12o: `std.collections` `map_strings` / `filter_strings` / `fold_strings`; corpus `std_collections/hof_str`.

### Generic collection higher-order helpers (2026-07-21)

- Resolved in C16: `std.collections` now exposes generic `map<T,R>`, `filter<T>`, and `fold<T,A>`; the Int/String helpers remain compatibility wrappers. The old zero-argument `map()` factory was renamed to `map_string_int()` because Aura does not yet support overloads by arity. Generic HOFs over arbitrary user-defined element types still depend on broader generic codegen coverage.

### Soft file read `tryReadFile` (2026-07-21)

- Resolved in C12p: `std.io.tryReadFile(path): String?` (null on missing/error); throwing `readFile` kept; runtime `aura_try_read_file`; corpus `std_io/try_read_file`. Full `Result` I/O still deferred.

### C10 first-class funs batch (2026-07-20)

- Resolved C10a–j: diagnostics polish, lambdas (expr/block), fun types, val captures (MVP), HOF helpers. Remaining: richer captures / env GC (see open debt).

### Generic class implements interface (2026-07-20)

- Resolved in C9a: `class Box<T> : Boxable<T>`; open implements type args; class mono subst for assignability; codegen tags/upcast/dispatch for mono implementors. Corpus `iface/generic_class_impl.aura`.

### Generic `Iterable<E>` implements (2026-07-20)

- Resolved in C8c/C8d: `implements TypeRef` with args; `Ty::InterfaceApp`; method subst; mono iface codegen; `std.collections.Iterable<E>`; for-in.

### Nested Array mono + element free (2026-07-20)

- Resolved in C8e/C8f: nested `Array<Array<T>>` mono order; free nested buffers on drop/clear/set.

### Generic Set + for-in collections (2026-07-20)

- Resolved in C8g/C8h: `Set<T>`; `Set.get(i)` duck for-in; `for (k in map.keys)`.

### HashMap String→Int (2026-07-20)

- Resolved in C8i: open addressing + `hash_string`; `hash_map()` capacity 16.

### Array field non-destructive bind (2026-07-20)

- Resolved in C8j: bind/assign from field is view; return still moves (C7c).

### Lock registry schema v0 (2026-07-20)

- Resolved in C8k: parse `version`/`source`/`checksum` inline tables; no fetch yet.

### Nullable primitive `Int?` / `Bool?` C emit (2026-07-20)

- Resolved in C7a: `aura_opt_i64` / `aura_opt_bool` tagged structs; null/wrap/coerce; `== null` via `.has`; `!!` / `?:`; Map.get returns `Int?`. Corpus `types/opt_prim.aura`.

### GC mark / free Array fields (2026-07-20)

- Resolved in C7b: `aura_gc_alloc_full` + per-class `dtor` (free Array buffers on sweep/shutdown) and `mark_extras` (mark Array-of-class field elems via `aura_gc_mark_ptr`). Corpus `class/gc_array_field.aura`.

### Multi-error collect deferred (2026-07-20)

- Resolved in C6h: body statements keep typechecking after an error; `SemaErrors` + CLI prints all. Corpus `diag/multi_error.aura`.
- C7g: declaration phase also collects (continue next decl); corpus `diag/multi_decl.aura`.

### Array fields shallow-copy on ctor/assign (2026-07-20)

- Resolved in C6i (partial): constructor and `var` field assign move from owner locals/params (zero source); reassign frees prior field buffer. Corpus `generic/array_field_move.aura`.

### GC mark does not walk Array-of-class locals (2026-07-20)

- Resolved in C6e (partial): `aura_gc_add_array_root` on Array-of-class locals/params; collect marks `data[0..len)`. Corpus `class/gc_array.aura`.

### Shallow GC mark only (2026-07-20)

- Resolved in C6a: store alloc size; worklist deep scan of pointer-sized slots in marked objects. Corpus `class/gc_deep.aura`.

### Array params not owners (2026-07-20)

- Resolved in C6b (partial): Array params own buffer; call site moves from owner idents. Corpus `generic/array_param_move.aura`.

### Array return binding not owner (2026-07-20)

- Resolved in C6d: `val b = f()` / assign from call that yields Array marks binding owner; free old on reassignment. Corpus `generic/array_return_own.aura`.

### No std.collections Map (2026-07-20)

- Resolved in C6f (partial): `Map` String→Int linear + `map()`; later C8a generic Map.

### `for-in` has no Iterable protocol (duck only) (2026-07-20)

- Resolved in C6c (partial): `for-in` on interface with `len(): Int` + `get(Int): E`; duck class path kept. Generic Iterable: C8d.

### Alpha target capability probing (2026-07-22)

- P6 now rejects native builds outside the published Linux/macOS target matrix
  and reports supported alternatives. Explicit cross targets, sysroot
  discovery, and system-library/linker probing remain deferred until target
  descriptors are modeled in `CompileOptions`.

### Alpha race instrumentation (2026-07-22)

- R3 now emits source-IDed local read/write hooks and source-tagged task,
  await, join, and channel boundaries in detector-enabled profiles. The
  runtime still records events without conflict suppression or stable report
  formatting; vector-clock refinement and actionable diagnostics remain R4.

### Async I/O suspension (2026-07-22)

- Await now resumes pending frames that are not blocked on a channel/I/O waiter
  by re-queueing them through the deterministic executor. Waiter-driven wakeup,
  live-local hoisting, and full async I/O continuation remain deferred.

### Registry archive publication wiring (2026-07-22)

- U1 now provides a deterministic gzip/tar archive primitive and SHA-256 helper,
  but no `publish`/dry-run CLI command consumes it yet. Next step: wire manifest
  and dependency validation plus upload preview/orchestration before claiming U4.

### String-return ownership metadata (2026-07-22)

- Codegen now frees only known allocating `String` call results and treats
  unknown/user/generic `String` returns as borrowed to avoid invalid frees. This
  can retain allocations longer than necessary. Next step: propagate explicit
  return ownership metadata through sema and call instantiations.

### Registry upload production compatibility (2026-07-22)

- U5 uses the frozen `/api/v1/publish` fixture contract and does not claim
  compatibility with an external production registry. Next step: standardize
  a signed, server-defined publish protocol before replacing this endpoint.

### Registry update activation deferred (U6, 2026-07-22)

- U6 performs metadata-only compatibility discovery and never downloads or
  activates a candidate. Signature verification, atomic replacement, rollback,
  and executable handoff remain U7 by dependency design.

### U8 cross-host release acceptance (updated 2026-07-27)

- The deterministic release-integration fixture now covers publish, install,
  checksum verification, discovery, activation, rollback, and execution on
  native Linux. The fixture and evidence writer now select Linux/Darwin target
  metadata from the actual host, and the acceptance script invokes the exact
  U8 test name. A native macOS run is still required before claiming macOS
  execution evidence; the fixture intentionally does not emulate another host.

### BUILD-002 / REL-003 target-host boundary (2026-07-23)

- Host-only validation now fails closed on target-manifest, workflow, package,
  and tier-2 policy drift; it intentionally does not claim foreign compilation
  or execution.
- Next step: run Linux arm64 and Windows package/native acceptance on their
  declared hosts before promoting BUILD-002/REL-003 from partial.

### F2 foreign failure and search-path integration (2026-07-22)

- Primitive foreign calls now lower and link against explicit C libraries on
  the native Linux/macOS matrix. The bounded `failure = "status"` convention
  maps an `Int` return through `aura_ffi_map_error`; undecorated returns remain
  ordinary values and missing symbols remain deterministic linker diagnostics.
- Package manifests do not yet expose foreign library search paths; the next
  step is target-aware package linker configuration for release packaging.

### F3 structured FFI values remain bounded (2026-07-22)

- The F3 ABI supports only malloc-backed String values and primitive byte,
  `int64_t`, and one-byte boolean arrays. String-element deep copy, arbitrary
  element destructors, pointers, callbacks, and foreign failure mapping remain
  deferred to F4/F5; the root guard is synchronous-only by contract.

### F3 structured foreign values (2026-07-22)

- F3 freezes an allocation-only C surface for borrowed/copied/transferred
  strings and primitive arrays, with synchronous GC root guards. String-element
  arrays, arbitrary destructors, pointers, callbacks, and async retention are
  intentionally deferred to F4/F5; extend the declaration model only after
  those lifetimes have a complete contract.

### F4 opaque foreign handles (updated 2026-07-28)

- F4 provides a tombstoned opaque-handle ABI with deferred destruction while
  pinned. `aura_ffi_handle_pin_for_boundary` now validates and retains a live
  handle through bounded TASK and AWAIT ownership windows. The public
  `aura_ffi.h` scheduler ABI now exposes exact-once TaskHandle payload and
  ChannelValue transfer helpers; Aura-level foreign declarations still reject
  arbitrary Task/Channel signatures until their source-level generic ABI is
  specified. Aura `Channel<ForeignHandle<T>>`
  payloads now retain on enqueue, transfer on receive, and drop queued refs
  exactly once. The compiler now fail-closes foreign declarations
  that expose `Task`, `TaskHandle`, or `Channel` (including nullable forms),
  while keeping primitive foreign declarations on the existing supported ABI.
  Tagged `ForeignHandle<T>` parameters use the checked pin/unpin ABI for
  synchronous calls and compiler-generated async pollers. Direct owned
  `ForeignHandle<T>` returns are now supported for foreign calls and bounded
  async task results: task-result destruction drops exactly once, owning joins
  retain one alias per observation, and owned `Result<ForeignHandle<T>,
TaskError>` locals release their payload at scope exit. Nested
  `ForeignHandle<ForeignHandle<T>>` values now reuse the outer opaque-pointer
  retain/drop contract; sema, emitted-C, and strict native fixtures cover
  parameters, direct returns, async task results, repeated owning joins, and
  nested destruction. CHANNEL/CALLBACK crossings and broader arbitrary CFG
  handle results remain deferred. Callbacks and foreign error mapping belong to
  F5.

### F5 callback portability (2026-07-22)

- F5 provides a single-threaded synchronous callback ABI with explicit
  environment ownership, frame retention, affinity checks, and bounded integer
  error mapping. It now also provides an explicit 16 MiB owned callback
  snapshot API with caller-supplied clone/destroy hooks and idempotent release;
  raw callback payloads remain borrowed by default. Concurrent foreign-thread
  delivery, host-specific callback trampolines, cancellation resumption, and
  exception-object translation remain deferred to cross-host acceptance work.

### F6 cross-host native acceptance (resolved 2026-07-23)

- The Linux native FFI matrix is now reproducible through
  `scripts/ffi-regression.sh`, covering owned values, opaque handles,
  callbacks, sanitizers, and the compiler primitive-call fixture.
- The script now accepts both Linux and Darwin native hosts, and CI schedules
  the same matrix on `macos-14` arm64 alongside Linux.
- The first Darwin run exposed and fixed static archive resolution: macOS
  `-force_load` now receives the resolved archive path rather than relying on
  `-L` search semantics.
- GitHub Actions run `29981605723` passed the Linux and Darwin native FFI jobs;
  the Darwin job ran on `macos-14` arm64 with the same compiler fixture.

### H7/IO6 Aura-level example remains deferred (2026-07-23)

- A bounded native health companion now proves localhost bind, `/health`
  exchange, malformed-request error mapping, async task progress, and
  deterministic shutdown through `runtime.c`; its smoke script records output
  and exit status under ASAN/UBSAN.
- A bounded `aura run examples/http-health-cli` entrypoint now calls
  the primitive `aura_http_health_smoke(): Int` bridge and is included in the
  sanitizer matrix. The full typed Aura TCP handle API and installed-release
  journey remain deferred.
- Next step: run the CLI entrypoint on macOS and replace the primitive bridge
  with documented typed bindings after the broader async API is frozen.

### Race report command integration (2026-07-22)

- R4 now has deterministic runtime reports and planted-race/suppression/
  synchronization fixtures; the user-facing race command, exit policy, and
  release/profile acceptance remain R5.

### Race CLI bounded evidence (2026-07-22)

- R5 adds `aura race` and a deterministic regression script over the bounded
  single-threaded report fixture. The command reports detector-enabled test
  outcomes; it does not yet stream runtime reports from arbitrary application
  binaries or provide concurrent vector-clock diagnostics. Those remain
  deferred until the runtime exposes a process-level report handoff.

### A4 async lowering boundary (superseded by G1, updated 2026-07-31)

- G1 now provides explicit CFG/state-machine lowering for its requested branch,
  loop, `for-in`, `break`/`continue`, `try`/`catch`, `match`, and arbitrary
  linear-await shapes, with native success/failure/cancellation/GC evidence.
  Remaining work here is ownership/capture and public outcome propagation,
  which belongs to G2 and is tracked by ASYNC-002/003/006.

### C5 corpus split scope (2026-07-23)

- The positive/negative corpus manifest covers the 14 `alpha-required` matrix
  rows. Partial and deferred rows remain outside the executable C5 claim until
  their dependencies are implemented; extend the manifest as each dependency
  closes.

### S2 bounded primitive capture (resolved 2026-07-31)

- The compiler copies `Int` values, boxes mutable `Int`/`Bool`/`String`/Array/
  class captures, heap-duplicates immutable `String` captures, roots class
  pointers, deep-clones supported arrays, and retains `Fun` environments in
  bounded spawn frames. A comprehensive native codegen fixture exercises every
  supported capture kind across `await`, forced GC, and repeated joins; the
  same fixture passes under ASAN/UBSAN. Cancellation and frame teardown are
  covered by the runtime matrix.
- Spawn frame discovery now infers unannotated `Int`, `Bool`, `String`, lambda,
  function-call, class-constructor, `Array<T>`, and sema-resolved expression
  locals before laying out the capture struct; native fixtures cover
  unannotated primitive, array, and expression captures.
- Spawn frame discovery now also accepts enum captures and uses generated
  enum clone/drop helpers for owned String/resource fields; an unannotated
  captured enum survives repeated owning joins and forced GC.
- The remaining limits are deliberate contract boundaries rather than
  unresolved capture bugs: richer aggregate nesting across suspension and
  broader scheduler policy remain tracked by ASYNC-003.

### RUNTIME-002 suspended frame ownership boundary (resolved 2026-07-26)

- Task-frame data is now rooted in the tracing heap for its lifetime; capture
  and pending storage receives a conservative pointer scan in addition to the
  explicit mark callback for opaque live state, with exact teardown coverage.
  Bounded spawn lowering now stores mutable Int/String/Bool/Array/Fun/class
  captures in shared refcounted boxes and releases the frame retain on
  destruction. Synchronous control flow, forced GC, and cancellation cleanup
  are covered by native tests. Owning calls clone boxed Arrays before callee
  teardown, while direct Array mutation continues to operate on the shared
  payload.
- Residual outside this contract: arbitrary async state-machine lowering and
  scheduler policies remain ASYNC-003 work; the shipped runtime frame ABI and
  generated capture boundary are complete for the alpha-required surface.

### examples/wc exit teardown (resolved 2026-07-23)

- `examples/wc` previously called `Io.exit(0)` after printing, bypassing the
  generated function teardown and leaking owned `Array<String>`/temporary
  strings under LeakSanitizer.
- The success path now returns normally; error paths retain explicit non-zero
  exits, and `scripts/install-smoke.sh --local-pkg` covers the regression.

### Aura HTTP primitive boundary remains synchronous (2026-07-23)

- `examples/http-health-aura` and `scripts/http-aura-smoke.sh` provide a
  runnable Aura-to-native HTTP status/response and loopback TCP fixture under
  sanitizers.
- Typed `AuraTcp*` handles, package-level `std.net` imports, async handler
  suspension, keep-alive, and response backpressure remain deferred to the
  full HTTP/FFI workstream; the contract matrix therefore stays `partial`.

### Alpha harness HTTP/FFI deferred stages resolved (2026-07-23)

- The harness now executes `scripts/http-aura-smoke.sh` and
  `scripts/ffi-regression.sh` instead of reporting those stages as deferred.
- The broader HTTP/FFI contract remains partial; this entry only records that
  the bounded acceptance stages are executable and sanitizer-backed.

### Registry production acceptance remains credential-gated (2026-07-23)

- Offline registry acceptance now validates publish receipts, update checksums,
  rollback, and explicit non-production/signature-deferred evidence.
- A live production registry, signing key, and network publish/update smoke are
  still unavailable in this environment; the validator intentionally rejects
  any report that claims those checks.

### ASYNC-002 task outcome representation (resolved 2026-07-31)

- Generated `join` now exposes `std.io.Result<T, std.io.TaskError>` and
  consistently maps failures to owned `TaskError.Failed(String)` and
  cancellation to `TaskError.Cancelled`. Primitive Int/Bool failures are
  normalized to owned strings, String failures preserve their detail, and
  no-await typed String plus heap-class spawn success is owned across repeated
  joins. Direct Array success is deep-cloned for owning joins; class results
  receive an independent GC root; ForeignHandle results retain a runtime
  reference; pending handle release cancels and reclaims the executor-owned
  frame synchronously.
- Class failures preserve their owned normalized message across nested awaits,
  forced GC, repeated owning joins, and lexical result cleanup. The raw class
  object remains internal to the frame ABI; `TaskError` intentionally exposes
  the canonical message-only failure contract.
- Progress (2026-08-02): nested async class failures now preserve a non-zero
  source identity derived from the throw span, in addition to the existing
  type name and span bounds. Async CFG and open-erased frames stamp their task
  source at construction, and the native metadata regression verifies both
  repeated joins after forced GC.
- The general CFG path now accepts caller-owned `Task<Int>` and `Task<String>`
  parameters, records `await_task_owned = false`, clones the String success
  payload across a branch/loop suspension, and preserves child failure and
  cancellation while repeated joins observe the parent. It now also retains
  and transfers `ForeignHandle<T>` values across caller-owned task awaits;
  typed handle results use an owned `Result.Ok` join payload. The public
  contract is intentionally canonical: `TaskError` exposes only
  `Failed(String)` and `Cancelled`; raw exception objects remain internal.

### ASYNC-003 post-G1 ownership frontier (updated 2026-07-31)

- G1's requested control-flow surface is complete; the residual below is
  ownership work rather than missing branch/loop lowering.
- The compiler now emits an explicit state graph for nested `if -> while -> await`
  and `while -> if -> await` shapes. Each await persists the graph state, child
  ownership bit, and live locals while propagating failure/cancellation across
  pending polls; native fixtures exercise forced GC, repeated joins, and queued
  cancellation.
- The same graph accepts `Int`, `Bool`, `String`, heap-class, and supported array
  values. String/array values are cloned at suspension and return boundaries;
  class results use the frame GC scan plus explicit terminal roots.
- Caller-owned `Task<String>` parameters now use the same graph and remain
  borrowed across nested awaits; native coverage proves success, failure,
  cancellation, forced GC, and repeated owning joins without static child
  release.
- Enum `match` statements, including supported pattern bindings, now lower to
  explicit tag branches in the same graph; native coverage proves branch
  selection, repeated joins, typed failure/cancellation, and forced GC.
  Aggregate pattern bindings now resolve generic enum arguments and clone/drop
  enum, value-struct, interface, function, foreign-handle, and heap-class
  payloads across the suspension frame. A native Packet binding fixture covers
  repeated joins; opaque aggregates without generated hooks remain rejected.
- Discarded `join(task)` statements now materialize an owned Result temporary
  and invoke its typed drop hook, covering nested aggregate success/error
  payloads instead of leaking or leaving a borrowed temporary.
  Pattern bindings,
  general CFG range loops now persist their iterator and bound across each
  await, including loop comparisons, loop back-edges, GC, and
  cancellation; a native fixture proves repeated owning joins. Await
  assignments now use the same graph, including nested loop/branch paths.
  Await failures from nested `if`/loop control flow route through a shared
  primitive/class catch continuation, and nested Array elements in `for-in`
  are cloned before suspension. Native fixtures cover success, failure,
  cancellation, forced GC, and an eight-await state machine. Remaining
  ownership frontier: owned enum aggregate values now clone/drop across general
  CFG await/return boundaries, and async frame GC marking now traverses enum
  fields containing heap classes, arrays of heap classes, or nested enums.
  Ordinary enum payloads remain borrowed unless their ABI marks ownership; the
  remaining boundary is the explicit rejection of opaque aggregates without
  generated clone/drop hooks.
- Progress (2026-08-01): interface values now have typed clone/drop/mark hooks;
  `Task<Interface>` owned joins use the canonical interface mono in
  `Result<T, TaskError>`, and interface payloads are cloned, dropped, and GC
  marked through nested enum/frame boundaries. Native repeated-join plus forced
  GC coverage exercises a heap-class implementation held behind an interface.
- Progress (2026-08-01): scheduler-owned `Task`/`TaskHandle`/`Channel` locals
  are now admitted to the general async CFG frame layout. Their frame drop
  hooks release the executor/channel ownership after suspension, and `cancel`
  actions can use the rematerialized handle after an awaited operation. Native
  coverage exercises a spawned handle kept live across `await`, cancellation,
  and terminal frame cleanup.
- Progress (2026-08-02): no-suspension async functions now apply the same
  scheduler ownership contract to `Task`, `TaskHandle`, and `Channel`
  parameters and results. They retain incoming payloads when the frame is
  created, release them in typed frame/result destructors, and retain returned
  scheduler values before publication. Native codegen coverage exercises both
  no-await and general-CFG result paths.
- Progress (2026-08-02): the general CFG fallback now retries with same-typed
  branch-local names merged into one frame slot after shape-specific lowerers
  decline. This permits aggregate locals in both arms without accepting
  incompatible type shadowing; a repeated-join `Packet` fixture covers it.
- Progress (2026-08-02): async CFG preprocessing now alpha-renames nested
  lexical bindings that shadow an outer name across branch, loop, match,
  try/catch, and nested spawn scopes. Distinct frame storage preserves the
  shadowing semantics through suspension; a native fixture covers an `Int`
  binding shadowed by `String` across an awaited branch. Lambda definitions
  remain on their existing whole-file capture path until their lexical
  environment is represented in the same scoped-name table.

### IO-002 compiler-generated descriptor I/O remains bounded (updated 2026-07-28)

- `std.io.readFd(fd, capacity)` now lowers to a compiler-generated frame with
  explicit fd wait/resume state, nonblocking read completion, owned String
  result/error storage, forced-GC retention, and cancellation cleanup. Joining
  a pending frame drives the executor's registered fd readiness.
- `std.io.writeFd(fd, content)` now copies its String input into the task frame,
  waits for `POLLOUT`, resumes after short writes, and returns an owned byte
  count. The generated fixture covers forced GC and cancellation cleanup.
- `std.io.readFile(file: ForeignHandle<Int>, capacity)` now has a compiler
  intrinsic that pins the borrowed opaque handle in the task frame, reads from
  its `AuraFile` resource, owns the result buffer, and unpins on terminal frame
  cleanup. `std.io.openFile(path, mode)` now creates the owned
  `AuraFfiOpaqueHandle`; native Aura fixtures cover construction, lexical
  cleanup, and async read execution.
- `std.io.writeFile(file: ForeignHandle<Int>, content)` now mirrors that pin
  lifetime, owns the input buffer, handles short writes through the frame wait
  state, and returns the transferred byte count. Bounded `spawn` now retains a
  captured `ForeignHandle` and drops the frame owner independently from the
  outer lexical binding; native coverage proves
  `openFile -> spawn { await writeFile(...) } -> join -> readFile`, while the
  runtime fixture proves the resource destructor runs once after both owners
  leave. Broader async caller capture remains deferred.
- A native Aura round-trip now executes `openFile -> spawn { await writeFile }
-> repeated join -> spawn { await readFile } -> repeated owning join`, with
  forced GC between suspensions and queued cancellation cleanup for a second
  reader. This proves the compiler-generated file operations beyond
  compile-only ABI checks.
- A general CFG async caller now retains `ForeignHandle<T>` parameters in its
  frame across multiple branch/loop awaits and drops that retain during frame
  teardown; native coverage forces GC and repeats owning joins while passing
  through a multi-await `writeTwice(file)` caller.
- A caller-owned `Task<ForeignHandle<Int>>` can now cross a CFG loop await and
  feed a second compiler-generated `readFile` await; a native fixture covers
  `writeFile -> reopen -> task-handle transfer -> readFile`, forced GC, and
  repeated owning joins.
- The runtime now has a regular-file async operation fixture: a real temporary
  file is opened, scheduled through the file readiness handle, resumed by the
  executor, read, and closed exactly once. This proves the POSIX regular-file
  path separately from pipe/TCP readiness; the existing combined I/O fixture
  remains outside the sanitizer seed list because its TCP leg is host-sensitive.
- `std.net.readStream` and `std.net.writeStream` now lower typed
  `ForeignHandle<Int>` values to task-pinned `AuraTcpStream` operations with
  readiness waits, EOF/error handling, short-write continuation, and terminal
  unpin cleanup. Compiler and native sanitizer fixtures cover the generated
  ABI, loopback construction, transfer, peer failure, and cleanup;
  `std.net.connect(port, timeout)` constructs an owned typed stream handle.
- The slice intentionally does not claim arbitrary aggregate caller capture or
  cross-platform reactor implementations. Portable regular-file and loopback
  behavior is covered for the currently supported POSIX hosts; the versioned
  `AuraReactor` policy boundary and POSIX implementation are now shipped.
  Keep IO-002 partial until those boundaries have typed Aura-facing contracts
  and native sanitizer coverage.

### ASYNC-004 primitive/string/array enum bindings in general CFG (bounded, updated 2026-07-28)

- `Int`/`Bool` enum pattern bindings persist in the async CFG frame, restore as poll locals, and synchronize across suspension/resume. `String` bindings are cloned into owned frame locals, while `Array<Int>`/`Array<String>` bindings use deep clone/drop helpers; native match-await fixtures cover forced GC and repeated owning joins.
- Primitive `throw` reached after a CFG `await` now becomes an owned
  task-frame failure with its source span, instead of longjmping through a
  poller that may have returned. The String path is covered by a real
  repeated-join/forced-GC regression.
- Aggregate bindings beyond supported arrays, binding ownership for richer element types, and `try`/richer pattern control flow remain unsupported; extend frame scan, clone/drop rules, and corpus before widening the contract.

### TIME-001 `std.time` timer surface is intentionally narrow (updated 2026-07-30)

- `std.time.sleep(Int)` now lowers to a monotonic runtime deadline wait with
  cancellation and range validation, and the Aura HTTP example exercises the
  real timer before responding.
- `std.task` now exposes the bounded existing lifecycle as `joinTask` and
  `cancelTask` around the builtin `TaskHandle<T>` and `std.io.Result<T,
TaskError>` ABI, plus cooperative `isCancelled()` inside generated async
  frames. `std.time.Duration`/`sleepFor` plus
  `nowMillis`/`Deadline`/`after`/`sleepUntil` now provide typed monotonic
  durations and relative deadlines. `std.task.cancelAfter<T>` composes a
  monotonic cancellation deadline with a live task handle, while `Instant`,
  wall-clock conversion/formatting, and structured parent/child cancellation
  remain open;
  `std.task.linkCancellation<P,C>` now links live same-executor handles so
  parent cancellation requests child cancellation and frame teardown unlinks
  the relationship deterministically. Full cancellation scopes, deadline
  propagation across arbitrary child sets, and graceful executor shutdown
  remain open; do not claim full time/task standard-library completion until those surfaces
  have public contracts and native acceptance fixtures.

### HTTP-003 installed CLI release gate (resolved 2026-07-29)

- A fresh offline release install under `/private/tmp/aura-install` passed
  `check`, `build`, and loopback smoke for GET `/health` (200), unknown target
  (404), and POST `/health` (405). The pre-existing `~/.local/bin/aura` remains
  stale and fails at runtime with `listen intrinsic`; it is not used as release
  evidence.

### REG-002 production trust remains open (updated 2026-07-23)

- Offline registry acceptance now verifies the versioned `aura-sig-v1` envelope
  with an explicit trusted key, rejects tampering and sequence replay, and
  fails closed when the keyring or signature is invalid.
- Production registry compatibility, credentialed network acceptance, and the
  release artifact's minisign key remain unavailable in this environment; the
  matrix therefore stays partial. Keep those claims separate from the offline
  metadata-signing evidence.

### ASYNC-005 generic async class methods remain bounded (2026-07-30)

- Area: async class-method monomorphization in the C backend.
- Progress: closed generic class monomorphs such as `Box<Int>` can lower an
  async method whose body and result do not depend on the open type parameter;
  synthetic `this` types and wrapper symbols use the concrete mono.
- Progress: `Box<Int>` and `Box<String>` async methods can return substituted
  `T` values after suspension; native regression coverage verifies the concrete
  wrappers and owned String result.
- Progress: `Box<Array<Int>>` now clones aggregate results without freeing a
  borrowed field alias; the sanitizer regression covers the prior double-free.
- Progress: nested `Box<Node>` async method calls now normalize short mono keys
  to package-qualified symbols and retain the nested class result through GC.
- Progress: nested `Box<Array<Node>>` results now recursively normalize short
  array/class keys and clone aggregate payloads across async boundaries.
- Progress (2026-08-01): closed generic async class-method bodies now undergo
  the same recursive type substitution before bounded/general CFG lowering,
  including local, catch, channel, lambda, and nested await annotations; the
  synthetic `this` wrapper keeps the concrete class mono in its frame layout.
- Progress (2026-08-01): generic free async declarations with no suspension
  points now record a separate async monomorph set, emit type-substituted task
  frame signatures, and compile/run through inferred `Task<T>` call sites. The
  sema body pass also restores the generic type-parameter scope before checking
  async bodies. Generic free async functions that suspend still require the
  general CFG emitter to carry mono arguments through every continuation.
- Progress (2026-08-01): generic free async functions are now closed before
  lowering, including parameter/return annotations, nested local/catch/channel
  types, lambda types, and all supported await CFG shapes. Int and
  `Array<String>` return payloads are covered by repeated native execution.
- Progress (2026-08-02): the runtime now exposes `AuraTypeErasedOps` and
  `AuraTypeErasedValue` clone/drop/mark callbacks plus frame-result transfer
  helpers. Codegen no longer represents an unresolved `TypeParam` as
  `int64_t`; it uses the explicit opaque shape, and an ASAN/UBSan ABI fixture
  verifies clone, mark, and exactly-once drop. Compiler lowering still needs
  descriptor-aware lowering for richer open bodies; the direct async identity
  shape now emits a frame with typed clone/drop/mark and erased result transfer
  across a `tick()` suspension, covered by a native codegen regression.
- Progress (2026-08-02): open generic async forwarding now passes the erased
  descriptor into a child open-generic task, retrieves the child's erased
  result before releasing it, and publishes the recovered descriptor with the
  same typed frame mark/drop hooks. Native compile/run coverage covers
  `forward<T>(value) -> identity<T>(value)` across suspension.
- Progress (2026-08-02): the descriptor-backed identity emitter now also
  handles the straight-line no-suspension form with the correct typed frame
  state cast; compile/run regression coverage prevents the generated C from
  referring to a nonexistent un-suffixed state typedef.
- Progress (2026-08-02): open generic async functions may now assign an awaited
  erased result to a `T` local and return that local. Operand lowering routes
  generic child calls through the erased symbol with the frame-owned descriptor
  value, then clone-extracts and drops the child result before publication.
  Native compile coverage verifies the frame uses no `T`-suffixed symbol or
  stack-only parameter after suspension.
- Progress (2026-08-02): erased task-result retrieval is now clone-out rather
  than a borrowed struct copy. Open-generic forwarding drops its previous
  descriptor before installing the cloned child value; the sanitizer fixture
  verifies the retrieved payload remains valid after child-frame destruction.
- Progress (2026-08-02): descriptor-backed and forwarding open-generic pollers
  now check the parent cancellation bit on every resume before creating or
  polling the next erased child. Cancellation stops multi-await generic chains
  at the frame boundary; the typed destroy hook releases any owned child handle.
- Residual: value-inspecting operations and richer nested generic aggregate
  construction still require either a closed monomorph or an explicit
  descriptor operation; the erased representation itself is complete for
  clone/drop/mark/forward transfer.
- Architecture audit (2026-08-03): open-generic async topology is now
  represented by symbolic MIR/state machines and strict unlowered diagnostics.
  The remaining value-inspection and aggregate operations are classified as
  alpha-runtime descriptor capabilities, not frontend lowering; non-C backends
  may reject them through the normal capability gate until they provide an
  equivalent runtime contract.
- Progress (2026-08-03): open generic async declarations now publish symbolic
  `Ty::TypeParam` MIR and MIR-derived state machines in `CheckedIr`; unsupported
  open shapes are tracked by an explicit unlowered list instead of disappearing
  into the C emitter. Descriptor clone/drop/mark remains a separate runtime
  capability boundary for the alpha backend.
- Progress (2026-08-03): the neutral `MirBackend` now serializes open-generic
  async bodies and state machines without C headers, erased runtime symbols, or
  compiler input; strict backend rejection still applies to unsupported shapes.

### ASYNC-006 async catch lowering remains bounded (updated 2026-08-01)

- Progress (2026-08-01): general CFG local initializers now lower one nested
  await in a supported call/binary/unary expression for aggregate result keys,
  using the same ownership-aware assignment helper as direct awaits. A native
  `String` call-argument continuation regression covers frame suspension,
  repeated join, and owned result cleanup.

- Progress: a single awaited task inside `try` can catch its owned `String`,
  `Int`, or `Bool` failure in top-level and class async methods; tagged child
  error storage is released exactly once after extraction. A bounded
  `val value = await task()` protected block now copies the successful value
  before entering its continuation, allowing typed wrappers to compile and
  run through the same catch state. Multiple sequential awaits in one
  primitive-catch region now route to a shared catch continuation, including
  value declarations, and are covered by a native runtime regression. The
  typed class catches now deep-copy String fields from the child error payload
  into an owned frame slot, release them with the generated exception dtor,
  and survive forced GC in a native regression. The bounded single-await
  finally path runs before propagating child failure. Array payloads can now be
  thrown and caught across an await through typed frame payload clone/destroy
  hooks, with sanitizer coverage for the recovered array length.
- Progress (2026-08-01): async class-error payload clones now retain nested
  heap-class fields in stable payload-owned GC root slots and remove those roots
  before the exception destructor frees the copy. A native catch-after-await
  regression forces GC and reads `Failure.child.value` successfully. Array-valued
  class fields now remain eligible for the general async CFG path, are cloned at
  throw/catch boundaries, and survive forced GC in a native regression.
- Progress (2026-08-01): class-error payloads containing `Array<HeapClass>` now
  register cloned buffers as explicit GC array roots; throw lowering removes
  local array roots before `longjmp`, and catch-frame cleanup unregisters copied
  roots before destruction. Native forced-GC coverage reads a child class from
  `Failure.values` after an awaited throw.
- Progress (2026-08-01): async typed failure now accepts enum, value-struct,
  interface, and function payloads in addition to scalar, Array, and class
  exceptions. Sema admission and CFG throw/catch lowering use generated
  clone/drop hooks; aggregate error payloads retain their type tag and survive
  nested await, forced GC, repeated join, and catch extraction. The native
  `async_enum_catch_after_await_with_forced_gc` fixture covers the enum path.
- Progress (2026-08-01): a protected awaited task may now combine a typed catch
  with a synchronous `finally`; success and matching failure both run the
  finally continuation, while an unmatched failure is retained and propagated
  after cleanup. Native coverage verifies `Int` catch plus finally after await.
- Progress (2026-08-01): parent cancellation is now routed through the active
  synchronous `finally` state for every protected CFG action/branch, not only
  for await nodes; generated states retain the cancellation bit and publish it
  after cleanup.
- Progress (2026-08-02): nested protected async blocks now compose their
  success/failure finally continuations; native coverage verifies inner cleanup
  runs before outer cleanup across two suspension points.
- Progress (2026-08-02): catch continuations may now suspend again after a
  typed failure. The CFG keeps the caught payload in its own owned frame slot,
  resumes through a second await, and releases the child task payload without
  borrowing a terminal frame; repeated execution plus forced GC is covered by
  `builds_and_runs_async_catch_that_suspends_again`.
- Progress (2026-08-02): general CFG expression statements may now contain
  awaits in nested call arguments and other supported expression positions;
  discarded values still receive typed frame slots and ownership cleanup.
  `builds_and_runs_async_statement_with_awaited_argument` covers the handler-
  style `consume(await value())` path.
- Progress (2026-08-02): `for-in` bindings now clone enum, value-struct,
  interface, and function elements before a suspension, and retain foreign
  handles before replacing the previous binding. Array `push`/`set` now use
  generated clone/drop hooks for aggregate elements; an `Array<Item>` enum
  binding regression runs through repeated joins and forced GC under ASAN.
- Progress (2026-08-02): aggregate `Array.get` now returns an owned clone for
  nested arrays, enums, value structs, interfaces, and foreign handles instead
  of exposing a shallow alias. This closes the direct read/closure-escape path
  that previously bypassed the `for-in` ownership fix.
- Progress (2026-08-01): each catch binding now receives a unique internal
  frame slot and a scoped source-name alias. Sequential catches may therefore
  reuse a source name with different payload types without C-field collisions;
  native coverage exercises `Int` then `String` catches named `error`.
- Resolved (2026-08-01): nested async `finally` cancellation now installs a
  typed frame cancel hook. The hook re-enters the CFG cancellation path so
  inner-to-outer synchronous finally blocks run before the runtime publishes
  `AURA_TASK_CANCELLED`; native coverage verifies the order and task cleanup.
  Nested protected branch/loop catch control flow remains covered by the
  general CFG path.

### ENCODING-001 bounded std.encoding contract (resolved 2026-07-30)

- `std.encoding` now provides UTF-8 validation plus Base64, hex, and percent
  encode/decode with native bounded allocation and malformed-input checks.
- The implementation intentionally rejects decoded NUL bytes because Aura
  `String` values are NUL-terminated; byte buffers remain a separate S04
  surface.

### URL-001 bounded URL/MIME surface (updated 2026-07-30)

- Progress: `std.url` validates origin-form request targets, extracts path/query
  strings, recognizes bounded absolute authorities, extracts userinfo-safe
  host/port components, and returns exact raw query values. `std.mime` validates
  media types, sanitizes path-like upload filenames, and extracts sanitized
  `filename` disposition parameters. Native and corpus fixtures cover the ABI.
- Progress: `std.url.encodeComponent`/`decodeComponent` reuse strict RFC 3986
  percent encoding and reject malformed escapes/NULs, covered by the URL/MIME
  corpus fixture.
- Progress: `std.url.normalizePath` removes bounded `.`/`..` path segments,
  preserves the root/trailing slash, and rejects query/fragment/control bytes.
- Residual: full RFC URL normalization, extended MIME disposition parameters,
  and multipart boundaries remain deferred to S09/H08.

### SYNC-001 bounded atomic surface (updated 2026-07-30)

- Progress: `std.sync.AtomicInt` uses sequentially consistent compiler atomics
  for load/store/fetch-add/compare-exchange and is covered by native and
  corpus fixtures.
- Progress: `std.sync.RwLock` now uses a CAS state (`0` unlocked, positive
  reader count, `-1` writer), with non-blocking read/write acquisition and
  explicit unlock methods covered by native codegen and corpus fixtures.
- Residual: atomic Bool/pointer variants, async lock cancellation, and
  lock-order/deadlock diagnostics remain open. `Mutex`/`Once` are intentionally
  non-blocking CAS primitives until scheduler-aware adapters exist.

### BYTES-001 bounded owned byte-string surface (updated 2026-07-30)

- Progress: `std.bytes` provides native owned `copy`, `concat`, bounded `slice`,
  and byte-wise `equals`, covered by codegen and corpus fixtures.
- Progress: `std.bytes.Buffer` owns an `Array<Int>`, validates byte range,
  supports nullable indexing and deep cloning, and is covered by native and
  corpus fixtures.
- Residual: zero-copy views, raw descriptor-backed buffers, and richer async
  stream adapters remain deferred to S04.

### STREAM-001 bounded async stream adapters (2026-07-30)

- Progress: embedded `std.stream.Reader` and `Writer` classes own a TCP handle,
  expose async `read`/`write`, and provide idempotent close methods. Corpus
  typecheck and native build cover class-method suspension and handle fields.
- Residual: raw byte buffers, zero-copy views, richer typed stream errors,
  backpressure controls, and general reader/writer trait adapters remain open.

### FS-001 bounded path helper surface (updated 2026-07-30)

- Progress: `std.fs` exposes owned portable path decomposition and joining
  helpers with native ABI and corpus coverage.
- Residual: richer directory iteration, process APIs, and typed platform
  errors remain deferred to S05.

### OS-001 bounded process/environment surface (updated 2026-07-30)

- Progress: `std.os` provides owned environment lookup/mutation, cwd, pid, and
  platform helpers with native and corpus coverage.
- Progress: `getEnvResult`, `setEnvResult`, and `unsetEnvResult` map bounded
  environment failures to shared `std.error.Outcome` values; the process
  corpus covers missing-variable and mutation paths.
- Residual: process spawning/wait, signals, metadata, unsupported-target
  results, and richer typed OS errors remain deferred to S05/S01.

### ERROR-001 shared stdlib error surface remains bounded (updated 2026-07-30)

- Progress: embedded `std.error` provides common `ErrorKind`, owned `Error`,
  and generic `Outcome<T, E>` values; bounded HTTP client framing returns that
  typed outcome through additive APIs and a corpus fixture exercises package
  embedding plus native execution.
- Progress: import-safe `invalidInput` and `notFound` constructors now cover
  bounded platform/environment wrappers without exposing enum case syntax.
- Progress: `std.error.kindCode` maps common POSIX errno/status values into
  stable category IDs and is exercised by the shared error corpus fixture.
- Progress: `std.error.Error.isRetryable` provides a conservative transient
  retry policy for I/O, network, and timeout categories.
- Progress (2026-08-01): `std.error.transport` maps bounded runtime phrases to
  canonical `TimedOut`, `Cancelled`, and `Disconnected` errors while retaining
  generic network fallback; native execution covers timeout/cancel/close and
  unknown diagnostics.
- Residual: transport-specific payloads and
  unifying the nominal name with `std.io.Result` remain open. The backend's
  merged-package resolver currently misbinds duplicate generic enum names, so
  `Outcome` is intentionally used until that resolver is fixed.

### SAN-003 async HTTP sanitizer matrix remains bounded (updated 2026-07-30)

- Evidence: `bash scripts/sanitizer-smoke.sh` passes the current deterministic
  ASAN/UBSAN seed manifest, including Content-Length/chunked/keep-alive HTTP
  parser fuzz, HTTP hardening, async
  disconnect/timeout, HTTP health, task GC/cancellation, async I/O, stdlib
  JSON/URL/MIME parser fuzzing, and FFI.
- Residual: this does not cover TLS/HTTP2/HTTP3/WebSocket/multipart paths that
  are not implemented, nor a cross-target sanitizer run; slowloris and
  decompression-bomb cases remain separate X02 work.

### DNS-001 resolver surface remains bounded (2026-07-30)

- Progress: `std.dns.resolveHost` selects one numeric IPv4/IPv6 address with
  family preference and owned nullable return storage; native codegen and
  `corpus/std_dns/resolve` cover literal-address resolution.
- Progress: `resolveHostResult` exposes lookup failure through shared
  `std.error.Outcome` network errors, covered by the DNS corpus fixture.
- Progress: `resolveHostList` returns a preference-ordered newline-delimited
  numeric address snapshot with a 64 KiB cap and native corpus coverage.
- Residual: asynchronous cancellation/timeout and service-name resolution
  remain deferred.

### NET-001 TCP surface remains bounded (2026-07-30)

- Progress: `std.net` exposes non-throwing async read/write wrappers using
  shared `std.error.Outcome` values; the typed corpus fixture covers package
  resolution and the native build path.
- Progress (2026-08-01): listener bind, stream connect, and idempotent close
  now have additive `Outcome<..., NetError>` wrappers; the legacy throwing and
  Bool forms remain compatibility shims.
- Progress (2026-08-01): typed net/http wrappers now pass runtime diagnostics
  through `std.error.transport`, preserving canonical timeout, cancellation,
  and disconnected categories; a native codegen regression covers all four
  classification branches and retry behavior.
- Residual: address-list iteration, richer timeout/cancellation payloads, and
  cross-platform transport error mapping remain deferred.

### ERROR-002 generic Outcome nested ownership remains bounded (2026-07-30)

- Area: async functions returning generic `std.error.Outcome` values with
  owned String/class payloads.
- Progress: non-unit async catch lowering now preserves the awaited value and
  allows the typed TCP wrappers to compile and run through success/failure
  continuations. `std.error.Outcome<String, Error>` success values use a
  cloning owned constructor and deep-clean their nested String on result/frame
  destruction. The `OutcomeErr(Error)` branch now carries an ownership bit;
  local bindings root the Error object and scope cleanup removes that root, while
  general CFG async frames register the same cleanup contract. Native coverage
  forces GC before observing the class error and compiles an awaited Outcome
  producer with the generated frame drop hook.
- Progress (2026-08-01): aggregate `Outcome` returns no longer register GC roots
  on stack temporaries. Enum clone paths defer nested class rooting to the
  receiving owner, and moved source roots are removed before the source local
  leaves scope. ASAN coverage now passes both direct class-error cleanup and
  repeated owning joins of `Outcome<String, Error>` with forced GC.
- Progress (2026-08-01): value-struct clone/constructor paths now also avoid
  registering nested heap-class fields as roots on stack destinations. The
  native repeated-join regression for `Result<Packet, TaskError>` where
  `Packet` contains a `Child` class passes with forced GC; the struct mark hook
  keeps the nested object live while the result/frame owns the aggregate.
- Progress (2026-08-01): enum fields can now reference value structs because
  class/struct headers are registered before enum payload resolution. Generic
  result layout ordering also defers `Result<Envelope, TaskError>` until an
  `Envelope` whose payload is a value struct is complete. Enum mark hooks
  recurse through nested structs and `Array<Struct>` elements; native repeated
  joins of `Result<Envelope, TaskError>` with `Envelope -> Payload -> Child`
  survive forced GC.
- Progress (2026-08-01): generic enum joins now cover a mixed
  `Box<Child>` payload with both String and generic-class variants. Enum clone
  and mark hooks recurse through the monomorph, while heap-class drop no longer
  removes a root owned by the task frame or receiving aggregate. Repeated joins
  after forced GC pass natively.
- Progress (2026-08-01): nested arrays of generic enum payloads now use
  recursive generated Array clone/drop/mark hooks and repeated owning-join
  coverage; they no longer belong to this residual.
- Progress (2026-08-01): semantic task-payload validation now permits
  `ForeignHandle` nested in recursively owned arrays, structs, and generic enum
  variants. The generated enum clone/drop hooks retain/release the opaque
  handle, and a codegen ABI regression covers repeated owning `join` layouts.
- Residual: opaque or unresolved user-defined error payloads that have no
  generated clone/drop hooks still need a generalized ownership ABI; concrete
  scalar, Array, class, enum, value-struct, interface, and function payloads
  now use the typed frame contract. Same-name catches are covered by the
  scoped CFG binding contract in ASYNC-006.

### LOG-001 logging surface remains bounded (2026-07-30)

- Progress: `std.log` provides deterministic debug/info/warn/error prefixes on
  flushed stderr, covered by native codegen and `corpus/std_log/basic`.
- Progress: `infoFields`/`errorFields` render alternating key/value context with
  deterministic odd-field handling, covered by the log corpus fixture.
- Progress: `setMinLevel`/`minLevel` configure and inspect the process-local
  minimum emitted level while preserving deterministic stderr formatting.
- Residual: configurable sinks, metrics export, and signal-aware shutdown
  remain deferred.

### METRICS-001 counter surface remains bounded (2026-07-30)

- Progress: `std.metrics.Counter` provides atomic add/increment/get/reset with
  native codegen and `corpus/std_metrics/counter` coverage.
- Progress: `Counter.prometheus` emits one bounded text exposition sample.
- Residual: histogram/gauge types, labels, and process-wide aggregation remain
  deferred.

### TEST-001 test package remains bounded (2026-07-30)

- Progress: `std.test` exposes deterministic Bool/Int/String assertion helpers
  using native diagnostics, covered by codegen and `corpus/std_test/assertions`.
- Progress: `corpus/std_test/async` schedules a test task, awaits the real
  monotonic timer, and exercises all assertion forms before completion.
- Residual: async network fixtures, isolation, and sanitizer matrix control
  remain deferred.

### JSON-001 JSON package remains bounded (2026-07-30)

- Progress: `std.json.isValid` validates UTF-8 JSON values with strict strings,
  literals, numbers, arrays/objects, and 64-level nesting; `escapeString`
  returns owned quoted JSON text. Native/codegen and corpus fixtures cover it.
- Progress: `std.json.Value` owns validated raw JSON text and exposes raw and
  serializer accessors; `parse` and the value corpus cover valid/invalid input.
- Progress: `Value.kind` plus root-type predicates classify bounded object,
  array, string, number, bool, and null values; the native value corpus covers
  object/array classification.
- Progress: `errorOffset` reports the first invalid byte offset and `-1` for
  complete JSON values, covered by the value corpus.
- Progress: traversal (`get`, `at`, `asString`, `keys`), independent-copy,
  size/depth inspection, duplicate-key policy, bounded parse options, typed
  parse results, and `decode<T>` now have locked source placeholders and
  corpus type coverage. Placeholder calls fail explicitly and do not claim
  backend behavior.
- Residual: implement the owned node tree and its clone/aliasing ABI, enforce
  byte/depth limits during parsing, preserve source-order/duplicate-key
  semantics, and add reflection/derive-backed typed mappings. Serializer
  ordering remains part of the tree backend contract.

### SIGNAL-001 signal integration remains bounded (2026-07-30)

- Progress: `std.signal` installs SIGINT/SIGTERM handlers and exposes an
  idempotent shutdown flag/clear operation, covered by native codegen and
  `corpus/std_signal/shutdown`.
- Progress: generated `std.http.serve` now observes the flag, closes its
  listener, rejects new accepts, and drains tracked connection frames before
  completion.
- Progress: `runtime/tests/signal_shutdown.c` raises SIGTERM and SIGINT,
  verifies the flag, and verifies clear/re-arm behavior under the native test
  and sanitizer manifest.
- Residual: typed unsupported-target errors remain deferred.

### FS-001 filesystem metadata remains bounded (2026-07-30)

- Progress: `std.fs.isDirectory` and `fileMode` provide portable bounded
  `stat` queries (missing/regular/directory/other), covered by native codegen
  and `corpus/std_fs/paths`.
- Progress: `std.fs.permissions` returns the low nine POSIX permission bits with
  zero-on-error fallback and shares the same fixture coverage.
- Progress: `std.fs.modifiedMillis` returns portable second-resolution epoch
  milliseconds with a `-1` sentinel for missing metadata.
- Progress: `std.fs.listNames` returns a bounded 64 KiB newline-delimited
  directory snapshot with null-on-error behavior.
- Progress: `std.fs.isSymlink` performs a non-following `lstat` check on POSIX
  (false on unsupported/error paths), covered by the native fixture and
  `corpus/std_fs/paths`.
- Progress: `readTextResult` and `writeTextResult` provide shared typed
  `Outcome` wrappers over the bounded soft file primitives.
- Residual: richer directory iteration and detailed platform-error mapping
  remain deferred.

### ASYNC-004 generic spawn result ownership remains partial (2026-08-01)

- Progress (2026-08-01): the runtime result release primitive now removes a
  GC root and clears the ownership record even when an externally managed
  payload has no destroy callback. This closes a frame teardown leak without
  inventing a destructor for opaque storage; a sanitizer regression covers a
  rooted result released with a null callback. Channel `ERROR` remains
  caller-owned, while `OK`/`PENDING` transfer ownership.

- Progress (2026-08-02): `Channel<Bool>` now has a complete generated C ABI,
  including owned boxed transfer, `Opt_Bool` receive inference, close, and
  GC-safe cleanup. Native codegen coverage verifies a true value survives
  send/receive and is observed after the nullable boundary. The remaining
  primitive channel gap is now limited to nested nullable shapes; `Channel<Unit>`
  uses an explicit zero-sized runtime token and has native send/receive/close
  coverage.
- Progress (2026-08-02): nullable aggregate channel elements now normalize to
  their underlying C representation for clone/drop/GC transfer. `Channel<Box?>`
  receives the additional nullable boundary as `Box??` without falling into
  the unsupported payload path; native ownership coverage forces the inferred
  value through channel close.
- Progress (2026-08-02): force-unwrapped nullable Array method receivers are
  materialized into owned expression temporaries before C lowering takes the
  receiver address. Read and mutating methods share this safe path, with
  repeated nullable Array task joins covered after forced GC.
- Progress (2026-08-02): scheduler-owned `Task`/`TaskHandle` payloads now use
  an executor-bound payload reference, and `Channel` values use a channel
  reference count. Generated send/receive and bounded spawn capture paths use
  these wrappers; the ASAN/UBSan `task_payload_refs` fixture covers lexical
  handle drop, channel transfer, and final payload release.
- Progress (2026-08-02): `runtime/aura_ffi.h` now declares the complete public
  executor/frame lifecycle needed by an external translation unit, including
  poll-state and task construction types. A separately compiled header ABI
  fixture links against `runtime.c` and verifies task transfer through a
  channel without including runtime implementation internals.
- Progress (2026-08-01): executor shutdown now detaches frames that still have
  scheduler-owned payload references before freeing the executor. Their final
  payload destructor releases the detached frame without dereferencing the
  invalidated scheduler; `task_payload_refs` covers a queued task payload
  destroyed after shutdown under ASAN/UBSan.
- Progress (2026-08-02): `Channel<String>` receive bindings now record the
  transferred payload as a String owner, so lexical cleanup releases the
  channel-owned allocation exactly once instead of treating it as a borrowed
  pointer.
- Progress (2026-08-02): general spawn bodies now reuse the async CFG lowering
  for branch/loop/repeated-await shapes. Static capture discovery keeps inferred
  immutable locals, including `Array<T>`, in the generated signature and frame;
  aggregate parameters are cloned/marked/dropped symmetrically across the
  suspension boundary. Same-typed branch-local bindings share one frame slot.
  Async CFG preprocessing now alpha-renames incompatible nested lexical
  shadowing across branch, loop, match, try/catch, and nested spawn scopes;
  lambda bodies remain on the separate whole-file closure emitter so capture
  metadata stays synchronized. Native regressions cover inferred Array capture,
  repeated await execution, and an Int/String shadow across suspension.
- Progress (2026-08-02): synthetic general-spawn nominal captures now preserve
  Aura's `Name@package` nominal order when rebuilding TypeRefs. Previously a
  captured class could be silently rejected during CFG emission while its
  call-site still referenced the missing synthetic function. A regression now
  captures an inferred class through a branch/await CFG, forces GC, and verifies
  the class remains readable after suspension.
- Progress (2026-08-02): general CFG spawns now carry mutable primitive and
  mutable Array captures as retained shared-box references. Poller assignments
  write through the shared cell, Array method receivers address the boxed
  payload, and frame teardown releases the box exactly once. Native regressions
  cover branch/await mutation for `Int` and `Array<Int>`.
- Progress (2026-08-02): mutable Array lambda capture now has an end-to-end
  rebinding regression: an escaping closure mutates the replacement owner after
  forced GC, and the original cell remains independently released through the
  shared retain/release contract.
- Progress (2026-08-02): inferred spawn capture discovery is now lexical-scope
  aware. Locals declared inside the spawn, loop bindings, match/catch bindings,
  and lambda parameters shadow outer names during capture analysis, preventing
  accidental frame fields or ownership edges for an unrelated outer variable.
  A native shadowing regression verifies the generated frame does not capture
  the outer binding.
- Progress (2026-08-02): the same shared-cell lowering now covers mutable
  `String` and heap-class captures in general CFG spawns. Native regressions
  cover mutation after suspension, forced GC, and terminal frame cleanup; the
  full codegen suite (271 tests) and sanitizer smoke matrix pass with these
  paths enabled.
- Progress (2026-08-02): general CFG spawn coverage now includes an inferred
  `Array<Int>` capture through a loop, nested branch, try/catch, and repeated
  awaits, plus an owned `Array<Int>` result observed through
  `Result<Array<Int>, TaskError>`. Native execution verifies the frame clone,
  suspension state, forced-GC retention, and owning join cleanup for this
  previously untested combination.
- Progress (2026-08-02): unannotated generic capture now also covers a nested
  `Array<Array<Box>>` aggregate returned from `identity<T>`, with recursive
  clone/root/drop behavior after forced GC and spawn join. The remaining
  generic-capture residual is limited to opaque values without generated hooks.
- Progress (2026-08-02): nullable reference-like task payloads now preserve
  their `Opt_*` semantic monomorph through `spawn`, `await`, `join`, Result
  layout, and match context while using the underlying C representation for
  clone/drop/mark operations. A repeated owning `join` plus forced-GC
  `Box?` regression passes, including queued cancellation; nullable `String?`
  repeated joins also pass. A nullable heap-class task result now has native
  repeated-join coverage through `Result<Box?, TaskError>` after a suspension.
  Nullable `Array<Int>` task results also clone/drop through repeated owning
  joins after suspension; inferred nullable await locals use the same
  representation normalization.

- Progress (2026-08-02): owning `join` now maps post-completion retain failures
  for direct `ForeignHandle`, `Channel`, and task-handle payloads to an owned
  `TaskError.Failed` result instead of silently returning `Ok(null)`. This
  preserves typed failure semantics when a scheduler/handle reference has
  become invalid between observation and ownership transfer.
- Progress (2026-08-02): `ForeignHandle<T>` is now accepted as an async
  throw/catch payload. General CFG lowering retains the handle into a typed raw
  error payload, clones it into the catch binding, and releases both the child
  payload and catch owner exactly once; native coverage crosses an await and
  forced frame cleanup.

- Progress (2026-08-01): general async CFG await nodes now preserve awaited
  child cancellation through synchronous `finally` blocks before publishing
  the cancellation outcome; unmatched typed failures use the same finally
  edge. Direct cancellation of the parent frame still follows the runtime's
  cooperative terminal-boundary contract; CFG frames with finally blocks now
  register a cancel hook so direct parent cancellation also runs every
  synchronous enclosing finally block before terminal publication.

- Progress: spawn-frame discovery now preserves generic return substitution for
  unannotated local initializers (including async generic calls), so captures
  are not dropped merely because the local type was inferred rather than
  written. Native codegen coverage exercises a generic `identity(41)` local
  captured by a spawned task.
- Progress (2026-08-02): closed generic monomorphs now participate in spawn
  frame discovery after substitution of parameter and call types. Capture
  frame, poller, destroy, and mark symbols include the concrete capture-key
  tuple, so distinct `Int` and `String` instantiations cannot alias storage.
  Transitive generic callees referenced from those closed bodies are discovered
  and emitted as well, including calls made inside a spawned capture body.
  Open value-dependent operations without generated ownership descriptors
  remain rejected rather than using an unsafe layout fallback.
- Progress: bounded generic spawns now publish inferred non-Unit return values
  through the task result ABI; nested Outcome<String, Error> returns transfer
  payload ownership and survive repeated owning joins and forced GC.
- Progress: owned join now accepts enum payloads whose variants contain only
  scalar/unit fields, with native sanitizer coverage for repeated task result
  observation.
- Progress: generated enum monomorphs now expose clone/drop helpers for owned
  String fields; generic spawn and owned join use them, covered by repeated
  join plus forced-GC sanitizer execution.
- Progress: the same enum clone/drop boundary now handles nested Array, class,
  foreign-handle, and enum fields, including root/retain transitions.
- Progress: owned `join` now uses an explicit `Result.OkOwned` constructor for
  enum payloads, so cloned enum aggregates are dropped by lexical result
  cleanup instead of being left with the borrowed constructor bit.
- Progress (2026-08-01): generic result layout and mark traversal now cover an
  enum payload containing a value struct with a nested heap class, including
  repeated owning joins and forced GC. Enum payload resolution now sees local
  structs before the enum field pass.
- Progress: generated enum mark helpers now let async frame GC traverse nested
  heap-class payloads without treating borrowed enum fields as owned.
- Progress: value-struct clone paths no longer leave stale roots for nested
  heap-class fields copied through stack temporaries; struct mark hooks and
  result/frame ownership provide the liveness edge, covered by a repeated
  `Result<Packet, TaskError>` join regression with forced GC.
- Progress: typed `join` now handles `Result<Array<Enum>, TaskError>` end to end:
  C typedef emission is dependency-ordered, `Array<Enum>` clone/drop hooks are
  available before generic Result layouts, and `OkOwned` transfers ownership
  across repeated joins with forced GC coverage.
- Progress: compiler-owned Array cleanup now receives `CheckedFile` where
  available and recursively drops direct enum elements in lexical owners and
  enum/Result payloads, closing the clone-without-drop leak for `Array<Enum>`.
- Progress: the no-suspension async lowering now installs a typed frame mark
  hook and roots/drops every heap-class parameter, including Array-of-class
  storage, rather than special-casing only `this`. A scope-escape regression
  forces GC after the caller's local is gone and before the frame is polled.
- Progress (2026-08-01): nullable primitive task results now use the same typed
  `Task<T>`/`join` ABI through bounded spawn suspension and repeated owning
  joins; `Opt_Int` and `Opt_Bool` remain scalar, non-rooted payloads.
- Progress: class GC mark-extras now traverses direct heap-class and enum fields
  as well as Array-of-enum fields, so an asynchronously captured parent object
  keeps nested aggregate references alive across forced collection.
- Progress (2026-08-01): the conservative async-frame GC scan now also traverses
  the separately stored raw `error_payload` used by nested typed failures.
  A sanitizer regression keeps a GC child reachable through that payload across
  collection and verifies exactly-once cleanup after frame destruction.
- Progress (2026-08-01): generic enum payloads containing nested
  `ForeignHandle` values now pass the same semantic transfer contract as direct
  handles; generated monomorph clone/drop hooks are emitted and verified.
- Progress (2026-08-01): bounded generic spawn pollers now register a typed
  frame GC mark hook for captured heap classes, arrays, interfaces, enums, and
  value structs; interface captures/results also use generated clone/drop
  hooks instead of the scalar fallback.
- Progress (2026-08-02): generic spawn result publication now deep-clones
  String and Array values returned through locals, and their terminal
  destructors dispatch to the typed ownership hooks. Repeated owning joins
  with forced GC cover an `Array<String>` local result.
- Progress (2026-08-02): async frame mark hooks now also traverse the typed
  terminal `Task<T>` result for heap classes, arrays, enums, value structs, and
  interfaces. This complements the runtime's conservative payload scan and
  keeps captured references alive after completion until the owning result is
  released; native struct/class and owned-array spawn regressions cover the
  contract.
- Progress (2026-08-02): Channel payloads now use generated typed clone/drop
  callbacks for enum, struct, interface, nested Array, and function-shaped
  aggregates. Native codegen regressions cover enum and `Array<String>`
  send/receive/close with forced GC; borrowed references remain rejected.
- Residual: future opaque runtime aggregates without a generated clone/drop or
  explicit scheduler reference ABI remain rejected.
- Progress: `Array<Enum>` now uses the enum's typed clone/drop hooks instead of
  `memcpy`, and enum element types are emitted by value in C. Native coverage
  exercises an enum carrying an owned String through array clone/clear/drop.
- Progress (2026-08-01): nested `Array<Array<T>>` now delegates clone/drop/mark
  to the inner generated Array hooks instead of shallow-copying the inner
  buffer. This covers nested arrays of enums/structs and preserves recursive
  ownership across repeated joins; a native regression exercises
  `Array<Array<Enum>>` with GC and repeated owning joins.
- Progress (2026-08-01): general multi-await result destructors now dispatch
  through generated class, enum, struct, interface, function, and Array
  ownership hooks instead of falling back to `free(data)`.
- Progress (2026-08-01): function-valued async results retain and release
  closure environments across frame results and owning `join`; a native
  `Task<(Int) -> Int>` regression covers the terminal cleanup path.
- Progress (2026-08-01): no-await and bounded multi-await enum/struct/interface
  results now clone borrowed constructor/field payloads before publishing the
  terminal frame result. This prevents typed drop hooks from freeing borrowed
  string fields or stack aliases; the existing repeated String-enum regression
  now passes under ASAN.
- Resolved (2026-08-01): generated synchronous try/catch wrappers now emit a
  typed C fallback for inferred non-Unit return types. This removes the
  `-Wreturn-type` warnings from the HTTP health/client generated programs
  without changing their terminal success or rethrow paths.
- Progress (2026-08-03): generic async class-method closure now lives in
  `aura-ir::generic_lowering::close_async_method`; the C backend retains only
  synthetic ABI naming and wrapper emission. Generic substitution of `this`,
  parameter types (including method-level type arguments), result type, and body is therefore shared with
  backend-neutral IR instead of being reconstructed in `class_emit.rs`; a
  direct semantic fixture covers the closed declaration.
