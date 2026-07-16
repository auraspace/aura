# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

### Type-param mono edge cases
- Area: codegen mono / generics
- Symptom: C4k fixed heap-class type params as pointers + field-chain method receivers; deeper nested mono inference may still default wrongly
- Why deferred: bounds.aura green
- Next step: audit remaining mono return-type inference
- Introduced: narrowed after C4k

### `for-in` has no Iterable protocol (Array + String only)
- Area: language / parser / sema / codegen (`ForInStmt`)
- Symptom: no custom iterables / `for (x in myCollection)`
- Why deferred: C3k Array + C3w String bytes; general protocol needs interface design
- Next step: Iterable protocol
- Introduced: narrowed after C3k/C3w

### `Array` shallow-copy free unsound
- Area: builtin Array free (C3t/C4r)
- Symptom: C4r frees on owner reassignment from Array(...); shallow copies still leak/double-free if misused
- Why deferred: full move/borrow
- Next step: move-only Array or borrow
- Introduced: C3j; free C3t; reassign C4r





### No registry / version resolve (path lock only)
- Area: toolchain / RFC-005
- Symptom: C3p/C4j write `aura.lock` for path deps including transitive; no semver, git deps, or registry
- Why deferred: path-only graph is enough for monorepo demos
- Next step: registry + version resolve
- Introduced: narrowed after C3p; nested paths C4j

### Array shallow free / enum elements
- Area: builtin Array
- Symptom: C4q allows struct by-value; enum still rejected; free buffer-only
- Why deferred: enum layout + ownership
- Next step: enum elements or move-only Array
- Introduced: narrowed after C4q


### Stdlib incomplete (collections)
- Area: stdlib / RFC-007
- Symptom: C4g/C4h cover std.io + std.assert; no Map/Set or Iterable std collections
- Why deferred: builtin Array is enough for demos
- Next step: collections package or Iterable protocol
- Introduced: narrowed after C4h

## Resolved

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
