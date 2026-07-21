# Aura roadmap

Living plan for docs, language specs, and the Rust toolchain. RFCs remain the design source of truth; this file tracks **execution order**.

| Field        | Value                                                                          |
| ------------ | ------------------------------------------------------------------------------ |
| **Updated**  | 2026-07-21                                                                     |
| **Strategy** | Dual-track: freeze MVP surface in RFCs while shipping vertical compiler slices |
| **License**  | MIT (see root `LICENSE`)                                                       |

## Status snapshot

| Track                       | Status                                                                                         |
| --------------------------- | ---------------------------------------------------------------------------------------------- |
| RFC static site (`site/`)   | Implemented; Cloudflare Pages → **https://aura.fadosoft.com**                                  |
| RFC-000 … RFC-013           | **All Accepted** — open questions resolved or Deferred (2026-07-16)                            |
| Language MVP                | RFC-001 §6.0 + post-C1; generic iface mono (C8c); Iterable (C8d); async/macros deferred        |
| Compiler                    | **C0–C11e** shipped (`v0.1.0-alpha`); **C12a–C12t Planned** (post-alpha usable CLI + captures) |
| Runtime / packages / stdlib | GC + nested Array free; Map/Set/HashMap(+resize); path lock; **std.io file I/O**               |

## Phases

```text
[P0] Ship docs site  →  [P1] Spec MVP  →  [P2] Compiler hello  →  [P3] Expand
```

### P0 — Docs site

- Parse and render RFCs from `docs/rfc/`
- Prerender static HTML; selective hydration (search, filter, theme, graph)
- Deploy to Cloudflare Pages (`VITE_BASE=/`, custom domain `aura.fadosoft.com`)

**Exit:** public URL + green deploy workflow.

### P1 — Spec MVP (Wave 1 content)

| Step | Work                                            | Exit                                                               |
| ---- | ----------------------------------------------- | ------------------------------------------------------------------ |
| 1.1  | Accept RFC-000                                  | Status **Accepted** ✓                                              |
| 1.2  | Freeze RFC-001 MVP surface (G0/G1)              | §6.0 + keyword list v0 ✓                                           |
| 1.3  | RFC-002 subset for nullability + nominal basics | Enough for `aura check` on corpus ✓                                |
| 1.4  | RFC-003: declare GC/tasks; defer runtime depth  | Explicit “not in C0/C1” ✓                                          |
| 1.5  | Corpus under `corpus/`                          | ≥10 programs; C0 parses them ✓                                     |
| 1.6  | Status review + lock open Qs (2026-07-16)       | All RFC-000…013 **Accepted**; remaining items Deferred or phased ✓ |

**Out of scope for P1 depth:** full 005–013, macros (010), reflection (009).

### P2 — Compiler bootstrap (C0 → C1)

Rust workspace (toolchain only; user language remains Aura):

| Milestone | Scope                                                              | Exit                                       |
| --------- | ------------------------------------------------------------------ | ------------------------------------------ |
| **C0**    | Lexer + recursive-descent parser + `aura check`                    | Done                                       |
| **C0+**   | Name resolution + light typecheck                                  | Done (`aura check` typechecks corpus)      |
| **C1**    | Codegen + runtime stub → native binary                             | Done via **C backend** + `cc` (LLVM later) |
| **C1b**   | Simple `class` + methods + `this` + field access                   | Done                                       |
| **C2a**   | `interface` + implements + closed-world dispatch                   | Done                                       |
| **C2b**   | Generics (class/fun type params + mono)                            | Done                                       |
| **C2c**   | Type-arg inference from args + expected type                       | Done                                       |
| **C2d**   | Nullability flow (`if (x != null)`) + `!!`                         | Done                                       |
| **C2e**   | Multi-param bounds / `where` clauses                               | Done                                       |
| **C3a**   | `struct` value types (fields + methods; no implements)             | Done                                       |
| **C3b**   | `enum` + `match` + generic `Result<T,E>` (exhaustive)              | Done                                       |
| **C3c**   | `throw` / `try` / `catch` / `finally` (String/Int/Bool)            | Done                                       |
| **C3d**   | `aura test` + `@test` + `assert` / `assert_eq`                     | Done                                       |
| **C3e**   | Multi-file same package + minimal `aura.toml`                      | Done                                       |
| **C3f**   | `import` + `pub` + path deps in `aura.toml`                        | Done                                       |
| **C3g**   | `throw` / `catch` class & struct values                            | Done                                       |
| **C3h**   | `for (i in start..end)` exclusive Int range                        | Done                                       |
| **C3i**   | `break` / `continue` in loops                                      | Done                                       |
| **C3j**   | Builtin `Array<T>` (`T` = Int/Bool/String; len/get/set)            | Done                                       |
| **C3k**   | `for (x in array)` over `Array<T>`                                 | Done                                       |
| **C3l**   | Inclusive range `for (i in a..=b)`                                 | Done                                       |
| **C3m**   | `Array.push` + capacity grow                                       | Done                                       |
| **C3n**   | `import path as Alias` + `Alias.fun(...)`                          | Done                                       |
| **C3o**   | Package-prefixed free-function C symbols                           | Done                                       |
| **C3p**   | `aura.lock` for path deps (verify + write)                         | Done                                       |
| **C3q**   | Bare comparison emit (no double parens / `-Wparentheses-equality`) | Done                                       |
| **C3r**   | `Array.pop` (empty throws)                                         | Done                                       |
| **C3s**   | Free `throw_obj` payloads on `aura_ex_clear`                       | Done                                       |
| **C3t**   | Free owned `Array` heap buffers at scope end / return              | Done                                       |
| **C3u**   | `import … as Alias` type qualify (`Alias.Type` / ctor)             | Done                                       |
| **C3v**   | Package-prefixed class/enum C symbols + multi-key tables           | Done                                       |
| **C3w**   | `for-in` over String (UTF-8 bytes as Int)                          | Done                                       |
| **C3x**   | GC MVP: `aura_gc_alloc` + shutdown free-all                        | Done                                       |
| **C3y**   | Class instances as GC heap references (`struct` by-value)          | Done                                       |
| **C3z**   | Minimal `std.io` package (`println` via path dep)                  | Done                                       |
| **C4a**   | Class identity `==` / `!=` (pointer; corpus)                       | Done                                       |
| **C4b**   | Nullable class `Class?` C emit + null flow                         | Done                                       |
| **C4c**   | `Array` of class heap references                                   | Done                                       |
| **C4d**   | Package-prefixed interface C symbols + multi-key                   | Done                                       |
| **C4e**   | String content equality (`strcmp`)                                 | Done                                       |
| **C4f**   | `Array.clear` (len=0, keep cap)                                    | Done                                       |
| **C4g**   | Auto-prelude `std.io` for packages                                 | Done                                       |
| **C4h**   | `std.assert` package + auto path for `std.*`                       | Done                                       |
| **C4i**   | Reject struct/enum/interface `==` in sema                          | Done                                       |
| **C4j**   | Nested path deps recorded in `aura.lock`                           | Done                                       |
| **C4k**   | Type-param heap class pointers + field method recv                 | Done                                       |
| **C4l**   | `else if` chaining (desugar to nested if)                          | Done                                       |
| **C4m**   | Null coalesce `?:`                                                 | Done                                       |
| **C4n**   | `Array.isEmpty`                                                    | Done                                       |
| **C4o**   | `Array.reserve(n)` (grow cap, keep len)                            | Done                                       |
| **C4p**   | `String.len` (UTF-8 byte length)                                   | Done                                       |
| **C4q**   | `Array` of struct by-value elements                                | Done                                       |
| **C4r**   | Free Array buffer on owner reassignment                            | Done                                       |
| **C4s**   | Safe call `?.` on nullable receivers                               | Done                                       |
| **C4t**   | `if` as expression (last branch expr; requires else)               | Done                                       |
| **C4u**   | Nested mono audit (open skip, return resolve, C forwards)          | Done                                       |
| **C4v**   | `String.isEmpty()` (byte length == 0)                              | Done                                       |
| **C4w**   | `String.charAt(i)` (UTF-8 byte as Int; OOB throws)                 | Done                                       |
| **C4x**   | Clear `Array` of enum/interface reject diagnostic                  | Done                                       |
| **C4y**   | Duck Iterable `for-in` (`len` + `get(Int)`)                        | Done                                       |
| **C4z**   | GC STW mark skeleton (roots + collect; free-all still)             | Done                                       |
| **C5a**   | `std/collections` stub package (Map/Set deferred)                  | Done                                       |
| **C5b**   | Array move on `val b = a` (zero source owner)                      | Done                                       |
| **C5c**   | Undefined-name `did you mean` suggestions                          | Done                                       |
| **C5d**   | Close C4u–C5d batch (debts/roadmap/plan Done)                      | Done                                       |
| **C5e**   | Array move on assign `b = a` (owner)                               | Done                                       |
| **C5f**   | GC collect mark+sweep when roots registered                        | Done                                       |
| **C5g**   | Codegen GC roots for heap-class locals/params/`this`               | Done                                       |
| **C5h**   | `String.startsWith`                                                | Done                                       |
| **C5i**   | `String.contains`                                                  | Done                                       |
| **C5j**   | `String.endsWith`                                                  | Done                                       |
| **C5k**   | Assign type mismatch expected/found                                | Done                                       |
| **C5l**   | Document Array non-owner shallow copy                              | Done                                       |
| **C5m**   | Builtin `gc_collect` + roots corpus                                | Done                                       |
| **C5n**   | Close C5e–C5n batch                                                | Done                                       |
| **C6a**   | Deep GC mark (scan object fields for nested GC ptrs)               | Done                                       |
| **C6b**   | Array move into function/method params                             | Done                                       |
| **C6c**   | Iterable protocol: `for-in` on iface `len()`+`get(Int)`            | Done                                       |
| **C6d**   | Array return/call binding owns buffer                              | Done                                       |
| **C6f**   | `std.collections` Map (String→Int) + Array-as-field emit           | Done                                       |
| **C6e**   | GC mark Array-of-class local/param buffers                         | Done                                       |
| **C6h**   | Multi-error collect in sema (body diagnostics batch)               | Done                                       |
| **C6i**   | Field Array ownership (ctor + var reassign move)                   | Done                                       |
| **C6g**   | Array of enum elements (by value; interface still rejected)        | Done                                       |
| **C6j**   | Close C6a–C6j batch (roadmap/debts)                                | Done                                       |
| **C7a**   | `Int?`/`Bool?` tagged optional C emit; `Map.get` → `Int?`          | Done                                       |
| **C7b**   | Array field GC: dtor free + mark_extras Array-of-class fields      | Done                                       |
| **C7c**   | Move-out Array field on return/bind/assign                         | Done                                       |
| **C7d**   | Plan + roadmap C7a–C7j                                             | Done                                       |
| **C7e**   | `std.collections` Set (String, linear)                             | Done                                       |
| **C7f**   | Map API expand (`remove` / `clear`)                                | Done                                       |
| **C7g**   | Multi-error collect in declaration phase                           | Done                                       |
| **C7h**   | Array-of-interface decision (reject MVP vs fat ptr)                | Done                                       |
| **C7i**   | Generic interfaces foundation (parse; implements mono deferred)    | Done                                       |
| **C7j**   | Array element drop: document defer (buffer-only free)              | Done                                       |
| **C8a**   | Generic `Map<K,V>`                                                 | Done                                       |
| **C8b**   | Path lock existence check + registry spike note                    | Done                                       |
| **C8c**   | Generic interface implements mono (`: Iface<T>`)                   | Done                                       |
| **C8d**   | `std.collections` `Iterable<E>` + for-in                           | Done                                       |
| **C8e**   | Nested `Array<Array<T>>` mono                                      | Done                                       |
| **C8f**   | Free nested Array element buffers                                  | Done                                       |
| **C8g**   | Generic `Set<T>`                                                   | Done                                       |
| **C8h**   | for-in over Map.keys / Set                                         | Done                                       |
| **C8i**   | `HashMap` String→Int open addressing                               | Done                                       |
| **C8j**   | Non-destructive Array field bind                                   | Done                                       |
| **C8k**   | `aura.lock` registry schema v0                                     | Done                                       |
| **C8l**   | Close C8c–C8l batch (roadmap/debts/plan)                           | Done                                       |
| **C9a**   | Generic class implements mono (`class Box<T> : Iface<T>`)          | Done                                       |
| **C9b**   | HashMap auto-resize on load                                        | Done                                       |
| **C9c**   | Builtin `Array.clone()`                                            | Done                                       |
| **C9d**   | String `+` concatenation                                           | Done                                       |
| **C9e**   | Expression-body functions `fun f(): T = expr`                      | Done                                       |
| **C9f**   | Type alias `type Name = T`                                         | Done                                       |
| **C9g**   | Top-level `const Name: T = literal`                                | Done                                       |
| **C9h**   | String interpolation `"hi ${name}"`                                | Done                                       |
| **C9i**   | `is` type test (class/interface)                                   | Done                                       |
| **C9j**   | Close C9a–C9j batch (roadmap/debts/plan)                           | Done                                       |
| **DX**    | line:col diagnostics with snippets                                 | Done                                       |
| **C10a**  | Plan + roadmap C10a–C10j                                           | Done                                       |
| **C10b**  | Diagnostics polish: context line + notes                           | Done                                       |
| **C10c**  | Parse lambdas `(x: T) => expr`                                     | Done                                       |
| **C10d**  | Sema `Ty::Fun` + call through fun value                            | Done                                       |
| **C10e**  | Codegen non-capturing lambdas (static C fn + fn ptr)               | Done                                       |
| **C10f**  | Fun type syntax `(T) -> U`                                         | Done                                       |
| **C10g**  | Lambda block body `(x) => { … }`                                   | Done                                       |
| **C10h**  | Lambda captures (`val` Int/Bool/String; fat-pointer Fun)           | Done                                       |
| **C10i**  | Higher-order helpers `map_ints` / `filter_ints` / `fold_ints`      | Done                                       |
| **C10j**  | Close C10a–C10j batch (roadmap/debts/plan)                         | Done                                       |
| **C11a**  | `std.io` file + console (`readFile`/`writeFile`/`appendFile`/…)    | Done                                       |
| **C11b**  | Fun capture-env ownership free (scope/move/return/param/for)       | Done                                       |
| **C11c**  | `aura new` / `aura init` + `version` scaffold                      | Done                                       |
| **C11d**  | `String.substring` + dogfood `examples/notes` + `this.method` fix  | Done                                       |
| **C11e**  | Embedded runtime + install docs + 0.1.0-alpha freeze               | Done                                       |
| **C12a**  | Plan + roadmap C12a–C12t (post-alpha batch)                        | **Done**                                   |
| **C12b**  | Program argv: runtime + `std.io.args(): Array<String>`             | **Done**                                   |
| **C12c**  | `aura run` / `test` pass-through args after `--`                   | **Done**                                   |
| **C12d**  | `std.io.readLine(): String?` (+ optional `readAllStdin`)           | **Done**                                   |
| **C12e**  | `std.io.exit(code: Int)`                                           | Planned                                    |
| **C12f**  | `String.indexOf`                                                   | Planned                                    |
| **C12g**  | `String.split` → `Array<String>`                                   | Planned                                    |
| **C12h**  | `String.trim` / `trimStart` / `trimEnd`                            | Planned                                    |
| **C12i**  | `String.toInt(): Int?`                                             | Planned                                    |
| **C12j**  | Join helper for `Array<String>`                                    | Planned                                    |
| **C12k**  | Lambda capture class (GC ptr + env mark)                           | Planned                                    |
| **C12l**  | Lambda capture Array (non-owning view MVP)                         | Planned                                    |
| **C12m**  | Lambda `var` Int/Bool capture by ref                               | Planned                                    |
| **C12n**  | `HashMap` String→String concrete                                   | Planned                                    |
| **C12o**  | String HOF helpers (`map_strings` / `filter_strings`)              | Planned                                    |
| **C12p**  | `tryReadFile(path): String?`                                       | Planned                                    |
| **C12q**  | Dogfood CLI (`examples/notes` argv or `examples/wc`)               | Planned                                    |
| **C12r**  | Corpus + guide sync for C12 surface                                | Planned                                    |
| **C12s**  | Dist/DX polish (install smoke; optional Windows best-effort CI)    | Planned                                    |
| **C12t**  | Close C12a–C12t batch (roadmap / debts / plan)                     | Planned                                    |

Plan: [`docs/plans/2026-07-21-next-20-c12a-c12t.md`](plans/2026-07-21-next-20-c12a-c12t.md).

**Out of scope C0/C1:** generics mono, async/tasks, macros, registry, incremental, LTO.

**Out of scope C12:** async/tasks, registry HTTP/semver, LLVM, true borrow, Array-of-iface, generic `HashMap<K,V>`, Fun-in-env capture, signed installers.

### P3 — Expand (after hello)

1. ~~Language surface C2–C10j~~ (funs/lambdas + HOF) → **C12k–m** richer captures → later: Fun capture, true borrow, Array-of-iface
2. Runtime: ~~alloc/GC + deep mark/sweep + nested Array free~~ + ~~Fun env free~~ → **C12b–e** process I/O → later: channels/tasks
3. Toolchain: ~~path deps + path lock + registry lock schema~~ → registry client / semver (RFC-005; **after C12**)
4. Stdlib: ~~io (console+file) + assert + Map/Set/HashMap(+resize) + Iterable~~ → **C12 String tools + HashMap str + tryReadFile** → later: generic HashMap
5. Cross targets + signed releases — **C12s** best-effort only

Write Wave 2–4 RFCs **as implementation needs them**, not all up front.

## Sprint discipline

Each sprint should ship:

1. One slice of **surface** (RFC amend or open-question resolve/defer)
2. One slice of **compiler** (tests green)
3. Corpus update when syntax changes

## Definition of done

| Phase | Done when                                                                 |
| ----- | ------------------------------------------------------------------------- |
| P0    | Pages URL + workflow green                                                |
| P1    | 000 Accepted; 001/002 MVP subset; corpus ≥10; open Q resolved or deferred |
| P2    | `aura build` produces a running hello binary                              |
| P3    | Feature-sized follow-ups (not one mega-PR)                                |

## Non-goals (near term)

- Full 80–120 page RFC-001 before a parser exists
- Package registry, macros, race detector before hello
- Self-hosting the compiler in Aura
- Site i18n / in-browser RFC editing

## Related docs

- [RFC index](rfc/README.md)
- [RFC-000 Vision](rfc/RFC-000-vision-design-principles.md)
- [RFC-001 Language](rfc/RFC-001-language-specification.md) — MVP §6.0
- [RFC-004 Compiler](rfc/RFC-004-compiler-architecture.md)
- [Site README](../site/README.md)
