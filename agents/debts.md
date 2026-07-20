# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

### `Array` field free on GC / return-from-field

- Area: builtin Array free (C3t…C6i)
- Symptom: C6i moves into class fields (ctor + var reassign); GC sweep still does not free Array buffers inside objects; returning a field Array shallow-copies (caller may free while object still holds pointer)
- Why deferred: no finalizers / field-borrow
- Next step: C6e or Array-in-GC story; move-out from fields if needed
- Introduced: narrowed after C6i (ctor/field assign move Done)

### No registry / version resolve (path lock only)

- Area: toolchain / RFC-005
- Symptom: C3p/C4j write `aura.lock` for path deps including transitive; no semver, git deps, or registry
- Why deferred: path-only graph is enough for monorepo demos
- Next step: registry + version resolve
- Introduced: narrowed after C3p; nested paths C4j

### Array shallow free / enum elements

- Area: builtin Array
- Symptom: C4q allows struct by-value; C4x clear reject for enum/interface elements; free buffer-only
- Why deferred: enum layout + ownership
- Next step: enum elements or move-only Array (C6g)
- Introduced: narrowed after C4q; message C4x

### Stdlib incomplete (collections)

- Area: stdlib / RFC-007
- Symptom: C6f `Map` is String→Int linear only; no Set, no generic Map
- Why deferred: no generic class APIs + hash yet
- Next step: generic Map or hash Map; Set
- Introduced: narrowed after C4h; stub C5a; Map C6f

### Nullable primitive `Int?` / `Bool?` C emit

- Area: codegen
- Symptom: `T?` for Int/Bool still uses bare int64_t/bool; `null` becomes invalid C
- Why deferred: need optional tag or sentinel convention
- Next step: tagged optional or reject nullable primitives with clear diag
- Introduced: noticed C6f Map.get

### GC mark: Array-of-class fields / nested only via object scan

- Area: runtime GC (C6e partial)
- Symptom: locals/params `Array` of heap class register `aura_gc_add_array_root`; **fields** that are Array-of-class still not scanned (deep mark sees malloc `data` pointer, not buffer elems)
- Why deferred: no type maps for fields
- Next step: register field Arrays or GC-typed buffers
- Introduced: narrowed after C6e

### Generic `Iterable<E>` interface

- Area: language / C6c
- Symptom: C6c is non-generic iface shape (`len`+`get`); element type fixed per interface
- Why deferred: interfaces have no type params yet
- Next step: generic interfaces then `Iterable<E>`
- Introduced: C6c

## Resolved

### Array fields shallow-copy on ctor/assign (2026-07-20)

- Resolved in C6i (partial): constructor and `var` field assign move from owner locals/params (zero source); reassign frees prior field buffer. Corpus `generic/array_field_move.aura`. GC free of field buffers still Open.

### GC mark does not walk Array-of-class locals (2026-07-20)

- Resolved in C6e (partial): `aura_gc_add_array_root` on Array-of-class locals/params; collect marks `data[0..len)`. Corpus `class/gc_array.aura`. Field Arrays still Open.

### Shallow GC mark only (2026-07-20)

- Resolved in C6a: store alloc size; worklist deep scan of pointer-sized slots in marked objects. Corpus `class/gc_deep.aura`.

### Array params not owners (2026-07-20)

- Resolved in C6b (partial): Array params own buffer; call site moves from owner idents. Corpus `generic/array_param_move.aura`. Fields still Open.

### Array return binding not owner (2026-07-20)

- Resolved in C6d: `val b = f()` / assign from call that yields Array marks binding owner; free old on reassignment. Corpus `generic/array_return_own.aura`.

### No std.collections Map (2026-07-20)

- Resolved in C6f (partial): `Map` String→Int linear + `map()`; Array-as-class-field emit order; field-chain type resolve; C keyword mangle; fun sig package context. Corpus `std_collections/app`.

### `for-in` has no Iterable protocol (duck only) (2026-07-20)

- Resolved in C6c (partial): `for-in` on interface with `len(): Int` + `get(Int): E`; duck class path kept. Corpus `control/for_in_iface.aura`. Generic Iterable still Open.

### Type-param mono edge cases / nested mono (2026-07-16)

- Resolved in C4u: skip open monomorphs (`Box_T`); expand nested concrete field monomorphs; method/fun return resolve substitutes type args; incomplete C struct forwards for nested mono order. Corpus `generic/nested.aura`.

### No String.isEmpty (2026-07-16)

- Resolved in C4v: `s.isEmpty()` → true when UTF-8 byte length is 0. Corpus `expr/string_isempty.aura`.

### No String.charAt (2026-07-16)

- Resolved in C4w: `s.charAt(i)` returns UTF-8 byte as Int; out of bounds / null throws. Corpus `expr/string_charat.aura`.

### Vague Array-of-enum diagnostic (2026-07-16)

- Resolved in C4x: dedicated message for `Array` of enum/interface; corpus `diag/array_enum.aura`. Enum elements still unsupported (see Open).

### for-in only Array + String (2026-07-16)

- Resolved in C4y (partial): duck Iterable — class/struct with `len` field or `len(): Int` plus `get(Int)`. Corpus `control/for_in_duck.aura`. Interface protocol: C6c.

### GC free-all only / no collect (2026-07-16)

- Resolved in C4z/C5f/C5g/C6a: roots + deep mark+sweep; codegen roots heap-class locals/params/`this`; `gc_collect()`. Array buffers still non-GC (see Open).

### No std.collections package path (2026-07-16)

- Resolved in C5a: `std/collections` stub + README; Map/Set still Open under Stdlib incomplete.

### Array `val b = a` double-free / UAF (2026-07-16)

- Resolved in C5b (partial): binding from an owning Array local moves ownership (zero source). Corpus `generic/array_move.aura`. Assign/params still Open.

### Array assign `b = a` shallow copy (2026-07-16)

- Resolved in C5e: assign from owning Array local moves (free old dst if owner, zero source). Corpus `generic/array_assign_move.aura`. Params still Open.

### No String.startsWith/contains/endsWith (2026-07-16)

- Resolved in C5h–C5j: prefix/substring/suffix predicates via strncmp/strstr/suffix strcmp. Corpora `expr/string_starts|contains|ends.aura`.

### Vague assign type mismatch message (2026-07-16)

- Resolved in C5k: expected/found for assign and annotated init.

### Array non-owner Ident copy (2026-07-16)

- C5l: still shallow when source is not an owner local (params/fields). Documented; move only for tracked owners (C5b/C5e).

### No gc_collect surface (2026-07-16)

- Resolved in C5m: builtin `gc_collect()` → `aura_gc_collect`; corpus `class/gc_roots.aura`.

### Undefined name with no typo hint (2026-07-16)

- Resolved in C5c: Levenshtein suggestion against locals/funs/types/aliases. Corpus `diag/undefined_typo.aura`. Multi-error collect still deferred.

### No if-expression (2026-07-16)

- Resolved in C4t: `if`/`else` as expr; branch value = last expression; requires else.

### No safe call `?.` (2026-07-16)

- Resolved in C4s: `?.` field/method on nullable receivers; short-circuit to null.

### Array owner reassignment leaked (2026-07-16)

- Resolved in C4r: free buffer before `a = Array(...)` on owner locals.

### No Array of struct (2026-07-16)

- Resolved in C4q: struct elements by-value in Array mono; corpus generic/array_struct.

### No String.len (2026-07-16)

- Resolved in C4p: `s.len` is UTF-8 byte length via strlen.

### No Array.reserve (2026-07-16)

- Resolved in C4o: `reserve(n)` grows capacity without changing len.

### No Array.isEmpty (2026-07-16)

- Resolved in C4n: `isEmpty()` returns len==0.

### No null coalesce `?:` (2026-07-16)

- Resolved in C4m: `T? ?: T` → non-null T; corpus `types/coalesce.aura`.

### No else-if chaining (2026-07-16)

- Resolved in C4l: `else if` desugars to nested if in else block; corpus `control/else_if.aura`.

### Bounded generic method this wrong for heap class (2026-07-16)

- Resolved in C4k: type-param substitution uses heap pointers; method calls on field chains resolve receiver type. `corpus/generic/bounds.aura` runs.

### Nested path deps not in aura.lock (2026-07-16)

- Resolved in C4j: lock records transitive path deps (`# transitive`); verify only requires direct toml entries.

### Struct/enum `==` failed at C compile (2026-07-16)

- Resolved in C4i: sema rejects struct/enum/interface equality with a clear diagnostic; class identity and String content remain.

### No std.assert package (2026-07-16)

- Resolved in C4h: `std/assert` with `assert` intrinsic; auto path resolve for `std.*` imports; `assert_eq` remains language builtin.

### No auto-prelude for std.io (2026-07-16)

- Resolved in C4g: package builds discover `std/io` (or `AURA_STD`) and inject `import std.io`; prefer std over builtins for free-fun resolve.

### No Array.clear (2026-07-16)

- Resolved in C4f: `clear()` sets len=0, keeps capacity; corpus `generic/array_clear.aura`.

### String equality was pointer identity (2026-07-16)

- Resolved in C4e: String `==`/`!=` use null-safe `strcmp`; class stays pointer identity.

### Interfaces unique by simple name (2026-07-16)

- Resolved in C4d: multi-key iface table + package-prefixed C mono; loader allows same name across packages; corpus `import/iface_app`.

### No Array of class (2026-07-16)

- Resolved in C4c: heap class elements as pointers; package mono for `Array_<Class>`; corpus `generic/array_class.aura`.

### Nullable class C emit wrong mono (2026-07-16)

- Resolved in C4b: `Class?` reuses package-aware class local type (heap pointer); corpus `corpus/class/nullable.aura`.

### Class identity `==` untested (2026-07-16)

- Resolved in C4a: class refs compare by pointer identity; corpus `corpus/class/identity.aura`.

### No stdlib prelude package (2026-07-15)

- Resolved in C3z: `std/io` package with `pub fun println` (intrinsic → `aura_println`); corpus path-deps it. Builtins remain for single-file hello.

### Classes are by-value C structs (2026-07-15)

- Resolved in C3y: user `class` ctor uses `aura_gc_alloc`; locals/params are pointers; methods take `this` pointer; `struct` stays by-value.

### `for-in` only Array (2026-07-15)

- Resolved in C3w: `for (b in string)` yields UTF-8 bytes as `Int`. Remaining: Iterable protocol under Open.

### Classes/enums still unique across packages (2026-07-15)

- Resolved in C3v: multi-key class/enum tables; `Name@pkg` nominal keys; package-prefixed C mono (`aura_cls_demo_lib_a_Token`); loader allows same simple name across packages.

### Import aliases: functions only; no type qualify (2026-07-15)

- Resolved in C3u: `Alias.Type` in type positions + `Alias.Type(...)` constructors; `TypeRef.qualifier` + package check.

### Array buffers never freed (2026-07-15)

- Resolved in C3t: locals initialized from `Array(...)` are freed at scope end and before return (value computed first). Remaining: element types + shallow-copy edge cases under Open.

### Exception object payloads leak heap copies (2026-07-15)

- Resolved in C3s: `owns_obj` on exception frame; `aura_ex_clear` frees `throw_obj` malloc after catch copies by value. Rethrow transfers ownership.

### No `Array.pop` (2026-07-15)

- Resolved in C3r: `pop()` returns last element, shrinks `len`; empty throws `"Array pop on empty"`.

### C equality emits extra parentheses (2026-07-15)

- Resolved in C3q: comparisons (`==`/`!=`/`</>`/…) emit without outer grouping parens so `if (x == y)` is not double-wrapped.

### No aura.lock for path deps (2026-07-15)

- Resolved in C3p: write/verify `aura.lock` against `aura.toml` [dependencies].

### Cross-package free functions require unique names (2026-07-15)

- Resolved in C3o: package-prefixed C symbols (`aura_fn_demo_math_square`) + multi-pkg fun table.

### Import aliases parsed but unused (2026-07-15)

- Resolved in C3n: `import path as Alias` → `Alias.fun(...)` free-function calls.
- Types: see C3u resolved entry.

### No `Array.push` / grow (2026-07-15)

- Resolved in C3m: `cap` field + `push` with doubling `realloc`.

### No inclusive range `..=` (2026-07-15)

- Resolved in C3l: `for (i in a..=b)` with `DotDotEq` token and `ForRangeStmt.inclusive`.

### `for` only exclusive Int ranges — no for-in (2026-07-15)

- Resolved in C3k: `for (x in array)` for `Array<T>` (`ForInStmt`); range form kept.
- Remaining: Array-only tracked under Open.

### No `break` / `continue` (2026-07-15)

- Resolved in C3i: loop-depth checked in sema; C `break`/`continue` in codegen.
- Commit: with debts log + C3i slice.

<!-- Move resolved items here with a short note and date. -->
