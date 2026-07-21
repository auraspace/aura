# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

> **Last closed batch:** [S2](../docs/plans/2026-07-21-s2-production-toolchain.md) (2026-07-21). Residual open items below.

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

### Lambda capture limits (MVP)

- Area: language / lambdas (C10h/C12k/C12l/C12m/C13e/C13f/C13g)
- Symptom: no `var` class/Array/Fun capture (val Fun + var Int/Bool/String OK)
- Why deferred: class/Array `var` needs richer box/GC protocol; `var` Fun still out
- Progress: fat-pointer Fun `{env,fn}`; copy-out of prim + **class GC ptr** + **Array header view** + **Fun nest env RC**; **`var` Int/Bool/String by shared refcounted box**; env `__drop` unregisters class roots / releases boxes / nested Fun envs then free (never frees Array buffers); corpus `lambda_capture.aura`, `lambda_capture_class.aura`, `lambda_capture_array.aura`, `lambda_capture_var.aura`, `lambda_capture_fun.aura`, `lambda_capture_var_str.aura`, `lambda_env_free.aura`, **`lambda_capture_stress.aura` (C13g mixed mark/free stress — no free/mark bugs)**
- Note (C12l): Array capture is a non-owning `{data,len,cap}` view (like field bind). Freeing/moving the outer Array owner while Fun is still live is **undefined**
- Note (C12m/C13f): `var` Int/Bool/String uses `aura_box_*` (refcount); String box owns heap copy (`set` frees previous); outer + each capturing env retain; multiple lambdas share mutations; escaping Fun keeps the box alive
- Note (C13g): Fun param transfer moves env (caller must not call after pass); nested retain via capture keeps both live — stress corpus documents both
- Next step: later `var` class/Array/Fun if needed
- Note: C12 batch closed (C12t); C13e Fun + C13f var String + C13g stress audit shipped — residual only
- Introduced: narrowed after C10h; env free 2026-07-20; class C12k 2026-07-21; Array view C12l 2026-07-21; var Int/Bool C12m 2026-07-21; Fun C13e 2026-07-21; var String C13f 2026-07-21; stress C13g 2026-07-21

### Array field return still moves (no true borrow type)

- Area: builtin Array (C7c/C8j)
- Symptom: `return this.items` still moves buffer out of the object; bind/assign from field is non-owning view (C8j)
- Why deferred: no `ref`/`borrow` type in the language; shallow view is enough for field reads
- Progress: C9c `Array.clone()` owning copy as escape hatch for field returns
- Next step: true borrow type if needed
- Introduced: narrowed after C8j; clone C9c

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

### Array of interface elements

- Area: builtin Array
- Symptom: interface elements rejected (C4x/C7h message); enum/class/struct/prim/Array OK
- Decision (C7h): **reject for MVP** — no `Array<I>` until a stable elem layout exists
- Why: interface values are closed-world fat/tag unions; Array mono needs fixed elem size today
- Next step (post-MVP): erase to fat pointer `(tag, data*)` or box each element as a class
- Introduced: narrowed after C6g; decision locked C7h

### Stdlib collections polish

- Area: stdlib / RFC-007
- Symptom: linear `Map`/`Set`; generic hash collections now cover HashMap and HashSet
- Why deferred: stdlib generic `map`/`filter`/`fold` API and corpus remain a separate task
- Progress: C9b auto-resize; C12n String→String; C12o String HOF; **C14** generic `HashMap<K,V>`; **C15** generic `HashSet<T>`; compiler support for generic HOF function types
- Next step: add stdlib generic `map`/`filter`/`fold` implementations and corpus coverage
- Note: C14/C15 resolved the generic hash-collection residual
- Introduced: narrowed after C8i; resize C9b; String→String C12n; String HOF C12o

## Resolved

### C16 generic HOF compiler support (2026-07-21)

- Resolved: sema accepts generic function parameters such as `(T) -> R`; codegen skips open generic `Fun<T, R>` typedefs and emits only concrete monomorphs, allowing generic `map`/`filter`/`fold` implementations to compile.

### C14 generic HashMap (2026-07-21)

- Resolved: compiler-backed `Hashable` for `Int`/`String`, generic open-addressing `HashMap<K,V>`, compatibility factories, Int-key corpus, and collection docs.

### C15 generic HashSet (2026-07-21)

- Resolved: generic open-addressing `HashSet<T : Hashable>` backed by `HashMap<T, Bool>`, String factory, Int-key corpus, iteration API, and collection docs.

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
