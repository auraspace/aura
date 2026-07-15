# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

### `for-in` only supports `Array` (no protocol)
- Area: language / parser / sema / codegen (`ForInStmt`)
- Symptom: no `for (x in string)` / custom iterables
- Why deferred: C3k desugars Array via `get`+index; general iterator protocol needs more types
- Next step: Iterable protocol or expand for-in to more collections
- Introduced: narrowed after C3k (was C3h range-only); `..=` done in C3l

### `Array<T>` limited element types and no free
- Area: builtin Array (C3j/C3m)
- Symptom: only `Int`/`Bool`/`String` elements; buffers from `calloc`/`realloc` never freed; no `pop`
- Why deferred: heap mono without GC; C3m added push/grow only
- Next step: GC-owned buffers or free on scope end; class elements as refs; `pop`
- Introduced: C3j; push resolved in C3m

### Exception object payloads leak heap copies
- Area: runtime / codegen (`aura_throw_obj`)
- Symptom: thrown class/struct values are `malloc`'d and never freed; no GC ownership
- Why deferred: C3g needed a working payload path without a GC
- Next step: free on `aura_ex_clear` when type is obj, or move to GC-managed heap
- Introduced: C3g (`29188ae`)

### Import aliases: functions only; no type qualify
- Area: sema / codegen (`import path as Alias`)
- Symptom: `Alias.fun(...)` works (C3n); no `Alias.Type` in type positions
- Why deferred: C3n shipped qualified free-function calls only
- Next step: qualified types/ctors
- Introduced: narrowed after C3n

### Classes/enums still unique across packages
- Area: package loader / codegen
- Symptom: free functions may share names (C3o); class/enum/interface simple names still collide at link
- Why deferred: C3o fixed functions only
- Next step: package-prefix type C symbols + multi-key class table
- Introduced: narrowed after C3o

### Path-dep graph only from `aura.toml` (no lockfile / registry)
- Area: toolchain / RFC-005
- Symptom: no `aura.lock`, no version resolve, nested deps need their own path entries or nested manifests
- Why deferred: C3e/C3f path deps only
- Next step: write/read minimal lockfile for path deps; registry later
- Introduced: C3e/C3f

### Classes are by-value C structs (not GC refs)
- Area: memory model / RFC-003 vs codegen
- Symptom: class identity/reference semantics incomplete; nullable class is partial; no heap identity
- Why deferred: C1b value-style structs/classes unblocked methods without GC
- Next step: GC MVP + class as pointer; keep `struct` by-value
- Introduced: C1b; still open after C3g

### C equality emits extra parentheses (compiler warnings)
- Area: codegen
- Symptom: `if ((t == INT64_C(10)))` triggers `-Wparentheses-equality` on clang
- Why deferred: cosmetic; does not affect correctness
- Next step: emit bare comparisons without double parens in conditions
- Introduced: noticed with C3h corpus

### No stdlib prelude package
- Area: stdlib / RFC-007
- Symptom: `println` / `assert` are compiler builtins, not `import std…`
- Why deferred: single-file hello path needed builtins first
- Next step: real `std` package + optional auto-prelude
- Introduced: C0–C1

## Resolved

### Cross-package free functions require unique names (2026-07-15)
- Resolved in C3o: package-prefixed C symbols (`aura_fn_demo_math_square`) + multi-pkg fun table.

### Import aliases parsed but unused (2026-07-15)
- Resolved in C3n: `import path as Alias` → `Alias.fun(...)` free-function calls.
- Remaining: type qualify under Open.

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
