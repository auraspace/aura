# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

> **Active batch:** [C13a–t plan](../docs/plans/2026-07-21-next-20-c13a-c13t.md) schedules work on several debts below (method-on-temp, Int→String, String array free, Fun/`var` String capture, registry K1). Update or remove entries when the matching C13 slice lands (prefer C13t batch pass).

### Lambda capture limits (MVP)

- Area: language / lambdas (C10h/C12k/C12l/C12m)
- Symptom: no nested Fun capture; no `var` String/class/Array capture
- Why deferred: Fun-in-env risks double free / shared-env GC; non-prim `var` needs richer box protocol
- Progress: fat-pointer Fun `{env,fn}`; copy-out of prim + **class GC ptr** + **Array header view**; **`var` Int/Bool by shared refcounted box**; env `__drop` unregisters class roots / releases boxes then free (never frees Array buffers); corpus `lambda_capture.aura`, `lambda_capture_class.aura`, `lambda_capture_array.aura`, `lambda_capture_var.aura`, `lambda_env_free.aura`
- Note (C12l): Array capture is a non-owning `{data,len,cap}` view (like field bind). Freeing/moving the outer Array owner while Fun is still live is **undefined**
- Note (C12m): `var` Int/Bool uses `aura_box_*` (refcount); outer + each capturing env retain; multiple lambdas share mutations; escaping Fun keeps the box alive
- Next step: Fun capture + shared-env GC; later `var` String/class/Array if needed
- Note: C12 batch closed (C12t); class/Array/`var` prim capture shipped — residual only
- Introduced: narrowed after C10h; env free 2026-07-20; class C12k 2026-07-21; Array view C12l 2026-07-21; var Int/Bool C12m 2026-07-21

### Array field return still moves (no true borrow type)

- Area: builtin Array (C7c/C8j)
- Symptom: `return this.items` still moves buffer out of the object; bind/assign from field is non-owning view (C8j)
- Why deferred: no `ref`/`borrow` type in the language; shallow view is enough for field reads
- Progress: C9c `Array.clone()` owning copy as escape hatch for field returns
- Next step: true borrow type if needed
- Introduced: narrowed after C8j; clone C9c

### No registry fetch / semver resolve

- Area: toolchain / RFC-005
- Symptom: path deps + lock; C8k parses registry pin form but does not fetch or resolve ranges; **build does not wire registry pins yet**
- Why deferred: monorepo path graph enough for demos
- Progress: C8b path existence; C8k `LockEntry` version/source/checksum schema; **RFC-005 updated (2026-07-21) — default registry is GitHub-backed**; **C13i** offline index client; **C13j** caret semver → lock pin; **C13k** tarball fetch + sha256 + extract to `AURA_REGISTRY_CACHE`/`~/.aura/registry/src` (local/`file://` only; no HTTP client)
- Next step: **C13l** wire lock pins into `aura build`/`check` from warm cache; then live HTTPS fetch polish, `github`/`git` deps (K1b), `aura publish` (K2)
- Introduced: narrowed after C3p; nested C4j; path check C8b; schema C8k

### Array of interface elements

- Area: builtin Array
- Symptom: interface elements rejected (C4x/C7h message); enum/class/struct/prim/Array OK
- Decision (C7h): **reject for MVP** — no `Array<I>` until a stable elem layout exists
- Why: interface values are closed-world fat/tag unions; Array mono needs fixed elem size today
- Next step (post-MVP): erase to fat pointer `(tag, data*)` or box each element as a class
- Introduced: narrowed after C6g; decision locked C7h

### Array element drop incomplete (non-Array elems)

- Area: builtin Array
- Symptom: C8f deep-frees nested Array elems on drop/clear/set; prim/class/enum/struct still buffer-only
- Why deferred: class elems are GC; prim/struct/enum need no free; only nested Array owned buffers
- Progress: C12g `String.split` allocates owned segment copies (`malloc`); C12h `trim`/`trimStart`/`trimEnd` also malloc owned copies (same MVP as `substring`/`+`); Array drop still frees only the pointer buffer (segment strings leak)
- Next step: free `const char *` elems that are known-owned, or adopt a shared string arena/RC; extend free loop if other owned buffer elems appear
- Introduced: narrowed after C7j; nested free C8f; split note 2026-07-21; trim note 2026-07-21

### Stdlib collections polish

- Area: stdlib / RFC-007
- Symptom: linear `Map`/`Set`; concrete HashMap monos only (no generic HashMap)
- Why deferred: String→Int + String→String cover demos; generic needs hashable protocol
- Progress: C9b auto-resize when load ≥ 1/2; explicit `grow()`; **C12n** `HashMapStr` String→String (`hash_map_str()`, `get` → `String?`); **C12o** `map_strings` / `filter_strings` / `fold_strings`
- Next step: later `HashMap<K,V>` when hashable protocol exists; generic map/filter when Fun mono polish lands
- Note: C12 batch closed (C12t); HashMapStr + String HOF shipped — residual is generic form only
- Introduced: narrowed after C8i; resize C9b; String→String C12n; String HOF C12o

### Chained method on `Array.get` temporary (codegen)

- Area: codegen / method recv
- Symptom: `arr.get(i).trim()` / `.toInt()` emits `aura_method_Unknown_*` and takes address of rvalue `const char *` → `cc` fail
- Why deferred: dogfood workaround is bind intermediate (`val s = arr.get(i); s.trim()`); full fix needs typed temporary for method recv on call results
- Hit by: C12q `examples/wc`
- Next step: emit a local for call-result receivers before method dispatch (String methods first)
- Introduced: 2026-07-21 (C12q)

### No std Int→String (CLI print)

- Area: stdlib / String
- Symptom: `"${n}"` interpolation is String-idents only (C9h); no `Int.toString` / format helper
- Why deferred: dogfood `examples/wc` ships a local `u64ToString` for count columns
- Next step: builtin or `std` decimal format (and optional Int in interpolation)
- Introduced: 2026-07-21 (C12q)

## Resolved

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
