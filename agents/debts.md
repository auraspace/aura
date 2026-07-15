# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

### `for-in` has no Iterable protocol (Array + String only)
- Area: language / parser / sema / codegen (`ForInStmt`)
- Symptom: no custom iterables / `for (x in myCollection)`
- Why deferred: C3k Array + C3w String bytes; general protocol needs interface design
- Next step: Iterable protocol
- Introduced: narrowed after C3k/C3w

### `Array<T>` limited element types; shallow-copy free unsound
- Area: builtin Array (C3j–C3t)
- Symptom: only `Int`/`Bool`/`String` elements; free tracks ctor-initialized locals only — shallow copies / pass-by-value can still leak or double-free if misused
- Why deferred: full ownership/move or GC; C3t frees owner locals at scope end / before return
- Next step: class elements as refs; GC-owned buffers; move-only Array or borrow
- Introduced: C3j; push C3m; pop C3r; free owners C3t



### Interfaces still unique by simple name
- Area: package loader / iface codegen
- Symptom: class/enum may share names across packages (C3v); interfaces still one simple name per link unit
- Why deferred: C3v focused class/enum multi-key + C mangling; iface tags/dispatch less urgent
- Next step: package-prefix interfaces + multi-key iface table
- Introduced: narrowed after C3v

### No registry / version resolve (path lock only)
- Area: toolchain / RFC-005
- Symptom: C3p writes `aura.lock` for path deps only; no semver, git deps, or registry
- Why deferred: path-only graph is enough for monorepo demos
- Next step: registry + version resolve; nested lock merge
- Introduced: narrowed after C3p

### Classes are by-value C structs (not GC refs)
- Area: memory model / RFC-003 vs codegen
- Symptom: class identity/reference semantics incomplete; nullable class is partial; no heap identity
- Why deferred: C1b value-style structs/classes unblocked methods without GC
- Next step: GC MVP + class as pointer; keep `struct` by-value
- Introduced: C1b; still open after C3g


### No stdlib prelude package
- Area: stdlib / RFC-007
- Symptom: `println` / `assert` are compiler builtins, not `import std…`
- Why deferred: single-file hello path needed builtins first
- Next step: real `std` package + optional auto-prelude
- Introduced: C0–C1

## Resolved

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
