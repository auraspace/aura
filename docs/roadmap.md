# Aura roadmap

Living plan for docs, language specs, and the Rust toolchain. RFCs remain the design source of truth; this file tracks **execution order**.

| Field        | Value                                                                          |
| ------------ | ------------------------------------------------------------------------------ |
| **Updated**  | 2026-07-22                                                                     |
| **Strategy** | Dual-track: freeze MVP surface in RFCs while shipping vertical compiler slices |
| **License**  | MIT (see root `LICENSE`)                                                       |

## Status snapshot

| Track                       | Status                                                                                      |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| RFC static site (`site/`)   | Implemented; Cloudflare Pages → **https://aura.fadosoft.com**                               |
| RFC-000 … RFC-013           | **All Accepted** — open questions resolved or Deferred (2026-07-16)                         |
| Language MVP                | RFC-001 §6.0 + post-C1; generic iface mono (C8c); Iterable (C8d); async/macros deferred     |
| Compiler                    | **C0–C20** closed: mutable captures plus collection snapshot contract/fixtures; live ownership deferred |
| Runtime / packages / stdlib | GC + String Array free; Map/Set/HashMap/HashMapStr; snapshot iterators; path+registry lock; std.io + process |
| Distribution contract       | **S2 complete:** Linux amd64, macOS arm64, macOS amd64; Windows amd64 deferred              |
| Release metadata            | **0.1.0-alpha published**; subsequent release work tracks changes after `v0.1.0-alpha`      |

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

| Milestone | Scope                                                              | Exit                                             |
| --------- | ------------------------------------------------------------------ | ------------------------------------------------ |
| **C0**    | Lexer + recursive-descent parser + `aura check`                    | Done                                             |
| **C0+**   | Name resolution + light typecheck                                  | Done (`aura check` typechecks corpus)            |
| **C1**    | Codegen + runtime stub → native binary                             | Done via **C backend** + `cc` (LLVM later)       |
| **C1b**   | Simple `class` + methods + `this` + field access                   | Done                                             |
| **C2a**   | `interface` + implements + closed-world dispatch                   | Done                                             |
| **C2b**   | Generics (class/fun type params + mono)                            | Done                                             |
| **C2c**   | Type-arg inference from args + expected type                       | Done                                             |
| **C2d**   | Nullability flow (`if (x != null)`) + `!!`                         | Done                                             |
| **C2e**   | Multi-param bounds / `where` clauses                               | Done                                             |
| **C3a**   | `struct` value types (fields + methods; no implements)             | Done                                             |
| **C3b**   | `enum` + `match` + generic `Result<T,E>` (exhaustive)              | Done                                             |
| **C3c**   | `throw` / `try` / `catch` / `finally` (String/Int/Bool)            | Done                                             |
| **C3d**   | `aura test` + `@test` + `assert` / `assert_eq`                     | Done                                             |
| **C3e**   | Multi-file same package + minimal `aura.toml`                      | Done                                             |
| **C3f**   | `import` + `pub` + path deps in `aura.toml`                        | Done                                             |
| **C3g**   | `throw` / `catch` class & struct values                            | Done                                             |
| **C3h**   | `for (i in start..end)` exclusive Int range                        | Done                                             |
| **C3i**   | `break` / `continue` in loops                                      | Done                                             |
| **C3j**   | Builtin `Array<T>` (`T` = Int/Bool/String; len/get/set)            | Done                                             |
| **C3k**   | `for (x in array)` over `Array<T>`                                 | Done                                             |
| **C3l**   | Inclusive range `for (i in a..=b)`                                 | Done                                             |
| **C3m**   | `Array.push` + capacity grow                                       | Done                                             |
| **C3n**   | `import path as Alias` + `Alias.fun(...)`                          | Done                                             |
| **C3o**   | Package-prefixed free-function C symbols                           | Done                                             |
| **C3p**   | `aura.lock` for path deps (verify + write)                         | Done                                             |
| **C3q**   | Bare comparison emit (no double parens / `-Wparentheses-equality`) | Done                                             |
| **C3r**   | `Array.pop` (empty throws)                                         | Done                                             |
| **C3s**   | Free `throw_obj` payloads on `aura_ex_clear`                       | Done                                             |
| **C3t**   | Free owned `Array` heap buffers at scope end / return              | Done                                             |
| **C3u**   | `import … as Alias` type qualify (`Alias.Type` / ctor)             | Done                                             |
| **C3v**   | Package-prefixed class/enum C symbols + multi-key tables           | Done                                             |
| **C3w**   | `for-in` over String (UTF-8 bytes as Int)                          | Done                                             |
| **C3x**   | GC MVP: `aura_gc_alloc` + shutdown free-all                        | Done                                             |
| **C3y**   | Class instances as GC heap references (`struct` by-value)          | Done                                             |
| **C3z**   | Minimal `std.io` package (`println` via path dep)                  | Done                                             |
| **C4a**   | Class identity `==` / `!=` (pointer; corpus)                       | Done                                             |
| **C4b**   | Nullable class `Class?` C emit + null flow                         | Done                                             |
| **C4c**   | `Array` of class heap references                                   | Done                                             |
| **C4d**   | Package-prefixed interface C symbols + multi-key                   | Done                                             |
| **C4e**   | String content equality (`strcmp`)                                 | Done                                             |
| **C4f**   | `Array.clear` (len=0, keep cap)                                    | Done                                             |
| **C4g**   | Auto-prelude `std.io` for packages                                 | Done                                             |
| **C4h**   | `std.assert` package + auto path for `std.*`                       | Done                                             |
| **C4i**   | Reject struct/enum/interface `==` in sema                          | Done                                             |
| **C4j**   | Nested path deps recorded in `aura.lock`                           | Done                                             |
| **C4k**   | Type-param heap class pointers + field method recv                 | Done                                             |
| **C4l**   | `else if` chaining (desugar to nested if)                          | Done                                             |
| **C4m**   | Null coalesce `?:`                                                 | Done                                             |
| **C4n**   | `Array.isEmpty`                                                    | Done                                             |
| **C4o**   | `Array.reserve(n)` (grow cap, keep len)                            | Done                                             |
| **C4p**   | `String.len` (UTF-8 byte length)                                   | Done                                             |
| **C4q**   | `Array` of struct by-value elements                                | Done                                             |
| **C4r**   | Free Array buffer on owner reassignment                            | Done                                             |
| **C4s**   | Safe call `?.` on nullable receivers                               | Done                                             |
| **C4t**   | `if` as expression (last branch expr; requires else)               | Done                                             |
| **C4u**   | Nested mono audit (open skip, return resolve, C forwards)          | Done                                             |
| **C4v**   | `String.isEmpty()` (byte length == 0)                              | Done                                             |
| **C4w**   | `String.charAt(i)` (UTF-8 byte as Int; OOB throws)                 | Done                                             |
| **C4x**   | Clear `Array` of enum/interface reject diagnostic                  | Done                                             |
| **C4y**   | Duck Iterable `for-in` (`len` + `get(Int)`)                        | Done                                             |
| **C4z**   | GC STW mark skeleton (roots + collect; free-all still)             | Done                                             |
| **C5a**   | `std/collections` stub package (Map/Set deferred)                  | Done                                             |
| **C5b**   | Array move on `val b = a` (zero source owner)                      | Done                                             |
| **C5c**   | Undefined-name `did you mean` suggestions                          | Done                                             |
| **C5d**   | Close C4u–C5d batch (debts/roadmap/plan Done)                      | Done                                             |
| **C5e**   | Array move on assign `b = a` (owner)                               | Done                                             |
| **C5f**   | GC collect mark+sweep when roots registered                        | Done                                             |
| **C5g**   | Codegen GC roots for heap-class locals/params/`this`               | Done                                             |
| **C5h**   | `String.startsWith`                                                | Done                                             |
| **C5i**   | `String.contains`                                                  | Done                                             |
| **C5j**   | `String.endsWith`                                                  | Done                                             |
| **C5k**   | Assign type mismatch expected/found                                | Done                                             |
| **C5l**   | Document Array non-owner shallow copy                              | Done                                             |
| **C5m**   | Builtin `gc_collect` + roots corpus                                | Done                                             |
| **C5n**   | Close C5e–C5n batch                                                | Done                                             |
| **C6a**   | Deep GC mark (scan object fields for nested GC ptrs)               | Done                                             |
| **C6b**   | Array move into function/method params                             | Done                                             |
| **C6c**   | Iterable protocol: `for-in` on iface `len()`+`get(Int)`            | Done                                             |
| **C6d**   | Array return/call binding owns buffer                              | Done                                             |
| **C6f**   | `std.collections` Map (String→Int) + Array-as-field emit           | Done                                             |
| **C6e**   | GC mark Array-of-class local/param buffers                         | Done                                             |
| **C6h**   | Multi-error collect in sema (body diagnostics batch)               | Done                                             |
| **C6i**   | Field Array ownership (ctor + var reassign move)                   | Done                                             |
| **C6g**   | Array of enum elements (by value; interface still rejected)        | Done                                             |
| **C6j**   | Close C6a–C6j batch (roadmap/debts)                                | Done                                             |
| **C7a**   | `Int?`/`Bool?` tagged optional C emit; `Map.get` → `Int?`          | Done                                             |
| **C7b**   | Array field GC: dtor free + mark_extras Array-of-class fields      | Done                                             |
| **C7c**   | Move-out Array field on return/bind/assign                         | Done                                             |
| **C7d**   | Plan + roadmap C7a–C7j                                             | Done                                             |
| **C7e**   | `std.collections` Set (String, linear)                             | Done                                             |
| **C7f**   | Map API expand (`remove` / `clear`)                                | Done                                             |
| **C7g**   | Multi-error collect in declaration phase                           | Done                                             |
| **C7h**   | Array-of-interface decision (reject MVP vs fat ptr)                | Done                                             |
| **C7i**   | Generic interfaces foundation (parse; implements mono deferred)    | Done                                             |
| **C7j**   | Array element drop: document defer (buffer-only free)              | Done                                             |
| **C8a**   | Generic `Map<K,V>`                                                 | Done                                             |
| **C8b**   | Path lock existence check + registry spike note                    | Done                                             |
| **C8c**   | Generic interface implements mono (`: Iface<T>`)                   | Done                                             |
| **C8d**   | `std.collections` `Iterable<E>` + for-in                           | Done                                             |
| **C8e**   | Nested `Array<Array<T>>` mono                                      | Done                                             |
| **C8f**   | Free nested Array element buffers                                  | Done                                             |
| **C8g**   | Generic `Set<T>`                                                   | Done                                             |
| **C8h**   | for-in over Map.keys / Set                                         | Done                                             |
| **C8i**   | `HashMap` String→Int open addressing                               | Done                                             |
| **C8j**   | Non-destructive Array field bind                                   | Done                                             |
| **C8k**   | `aura.lock` registry schema v0                                     | Done                                             |
| **C8l**   | Close C8c–C8l batch (roadmap/debts/plan)                           | Done                                             |
| **C9a**   | Generic class implements mono (`class Box<T> : Iface<T>`)          | Done                                             |
| **C9b**   | HashMap auto-resize on load                                        | Done                                             |
| **C9c**   | Builtin `Array.clone()`                                            | Done                                             |
| **C9d**   | String `+` concatenation                                           | Done                                             |
| **C9e**   | Expression-body functions `fun f(): T = expr`                      | Done                                             |
| **C9f**   | Type alias `type Name = T`                                         | Done                                             |
| **C9g**   | Top-level `const Name: T = literal`                                | Done                                             |
| **C9h**   | String interpolation `"hi ${name}"`                                | Done                                             |
| **C9i**   | `is` type test (class/interface)                                   | Done                                             |
| **C9j**   | Close C9a–C9j batch (roadmap/debts/plan)                           | Done                                             |
| **DX**    | line:col diagnostics with snippets                                 | Done                                             |
| **C10a**  | Plan + roadmap C10a–C10j                                           | Done                                             |
| **C10b**  | Diagnostics polish: context line + notes                           | Done                                             |
| **C10c**  | Parse lambdas `(x: T) => expr`                                     | Done                                             |
| **C10d**  | Sema `Ty::Fun` + call through fun value                            | Done                                             |
| **C10e**  | Codegen non-capturing lambdas (static C fn + fn ptr)               | Done                                             |
| **C10f**  | Fun type syntax `(T) -> U`                                         | Done                                             |
| **C10g**  | Lambda block body `(x) => { … }`                                   | Done                                             |
| **C10h**  | Lambda captures (`val` Int/Bool/String; fat-pointer Fun)           | Done                                             |
| **C10i**  | Higher-order helpers `map_ints` / `filter_ints` / `fold_ints`      | Done                                             |
| **C10j**  | Close C10a–C10j batch (roadmap/debts/plan)                         | Done                                             |
| **C11a**  | `std.io` file + console (`readFile`/`writeFile`/`appendFile`/…)    | Done                                             |
| **C11b**  | Fun capture-env ownership free (scope/move/return/param/for)       | Done                                             |
| **C11c**  | `aura new` / `aura init` + `version` scaffold                      | Done                                             |
| **C11d**  | `String.substring` + dogfood `examples/notes` + `this.method` fix  | Done                                             |
| **C11e**  | Embedded runtime + install docs + 0.1.0-alpha freeze               | Done                                             |
| **C12a**  | Plan + roadmap C12a–C12t (post-alpha batch)                        | **Done**                                         |
| **C12b**  | Program argv: runtime + `std.io.args(): Array<String>`             | **Done**                                         |
| **C12c**  | `aura run` / `test` pass-through args after `--`                   | **Done**                                         |
| **C12d**  | `std.io.readLine(): String?` (+ optional `readAllStdin`)           | **Done**                                         |
| **C12e**  | `std.io.exit(code: Int)`                                           | **Done**                                         |
| **C12f**  | `String.indexOf`                                                   | **Done**                                         |
| **C12g**  | `String.split` → `Array<String>`                                   | **Done**                                         |
| **C12h**  | `String.trim` / `trimStart` / `trimEnd`                            | **Done**                                         |
| **C12i**  | `String.toInt(): Int?`                                             | **Done**                                         |
| **C12j**  | Join helper for `Array<String>`                                    | **Done**                                         |
| **C12k**  | Lambda capture class (GC ptr + env mark)                           | **Done**                                         |
| **C12l**  | Lambda capture Array (non-owning view MVP)                         | **Done**                                         |
| **C12m**  | Lambda `var` Int/Bool capture by ref                               | **Done**                                         |
| **C12n**  | `HashMap` String→String concrete                                   | **Done**                                         |
| **C12o**  | String HOF helpers (`map_strings` / `filter_strings`)              | **Done**                                         |
| **C12p**  | `tryReadFile(path): String?`                                       | **Done**                                         |
| **C12q**  | Dogfood CLI (`examples/notes` argv or `examples/wc`)               | **Done**                                         |
| **C12r**  | Corpus + guide sync for C12 surface                                | **Done**                                         |
| **C12s**  | Dist/DX polish (install smoke; avm help; Windows amd64 deferred)   | **Done**                                         |
| **C12t**  | Close C12a–C12t batch (roadmap / debts / plan)                     | **Done**                                         |
| **C13a**  | Plan + roadmap C13a–C13t                                           | **Done**                                         |
| **C13b**  | Codegen: method recv on call-result temporary                      | **Done**                                         |
| **C13c**  | `Int.toString(): String` (+ optional Int interp)                   | **Done**                                         |
| **C13d**  | Free owned String elems in `Array<String>` drop                    | **Done**                                         |
| **C13e**  | Lambda capture Fun + env mark                                      | **Done**                                         |
| **C13f**  | Lambda `var` String capture (RC box)                               | **Done**                                         |
| **C13g**  | Capture env mark/free audit + stress corpus                        | **Done**                                         |
| **C13h**  | Reject diagnostics for unsupported `var` class/Array capture       | **Done**                                         |
| **C13i**  | Registry index client MVP (fixture + cache)                        | **Done**                                         |
| **C13j**  | Semver caret resolve → lock pins                                   | **Done**                                         |
| **C13k**  | Fetch tarball + sha256 + extract cache                             | **Done**                                         |
| **C13l**  | `aura build`/`check` with locked registry deps                     | **Done**                                         |
| **C13m**  | String ASCII `toLower` / `toUpper`                                 | **Done**                                         |
| **C13n**  | `std.io` `eprint` / `eprintln`                                     | **Done**                                         |
| **C13o**  | `tryWriteFile` soft write                                          | **Done**                                         |
| **C13p**  | Hashable / generic HashMap spike (docs)                            | **Done**                                         |
| **C13q**  | Dogfood: `examples/wc` polish                                      | **Done**                                         |
| **C13r**  | Corpus + guide sync for C13                                        | **Done**                                         |
| **C13s**  | Dist/DX: signing design note (option B)                            | **Done**                                         |
| **C13t**  | Close C13a–C13t batch (roadmap / debts / plan)                     | **Done**                                         |
| **C14**   | `Hashable` + generic `HashMap<K,V>` (mono, Int/String keys)        | **Done**                                         |
| **C18**   | Generic hash-collection snapshots and HOFs (HashMap/HashSet)       | **Done**                                         |
| **C19a**  | HashMap/HashSet accessors (`containsValue` / `containsAll`)        | **Done**                                         |
| **C19x**  | Substitute generic class constructors in generic bodies            | **Done**                                         |
| **C19y**  | Substitute nested generic return/local types                       | **Done**                                         |
| **C19b**  | `HashMapEntry<K,V>` shallow snapshots via `entries()`              | **Done**                                         |
| **C19c**  | Direct `for-in` over generic hash-map entry snapshots              | **Done**                                         |
| **C19d**  | Close C19 collection batch (guide / corpus / roadmap / debts)      | **Done**                                         |
| **C20c**  | Mutable `var` class capture (shared box + GC root)                 | **Done** — MVP; ownership caveats remain         |
| **C20d**  | Mutable `var` Array capture (shared box + captured Array payload)  | **Done** — MVP; live view/borrow safety deferred |
| **C20e**  | Mutable `var` Fun capture (shared box + nested env retention)      | **Done** — MVP; full ownership contract deferred |
| **C20f**  | Collection iterator and entry-view contract                       | **Done** — snapshots are the stable MVP; live views deferred |
| **C20g**  | Read-only collection iterator snapshots                            | **Done** — deterministic snapshot fixtures                         |
| **C20h**  | `Array<Interface>` layout spike                                   | **Deferred** — boxed layout recommended for a future batch           |
| **C20i**  | Collection mutation-through-entry                                 | **Deferred** — read-only snapshots avoid unsafe aliases              |
| **C20j**  | Close C20 documentation/status                                     | **Done** — C21 and release remain pending/deferred                   |

Plans:

- C12 (closed): [`docs/plans/2026-07-21-next-20-c12a-c12t.md`](plans/2026-07-21-next-20-c12a-c12t.md)
- C13 (closed): [`docs/plans/2026-07-21-next-20-c13a-c13t.md`](plans/2026-07-21-next-20-c13a-c13t.md)

**Out of scope C0/C1:** generics mono, async/tasks, macros, registry, incremental, LTO.

**Out of scope C12:** async/tasks, registry HTTP/semver, LLVM, true borrow, Array-of-iface, generic `HashMap<K,V>`, Fun-in-env capture, signed installers.

**Out of scope C13:** async/tasks, LLVM, true borrow, Array-of-iface, full generic HashMap, K1b `github=` / K2 publish, notarized installers (design note optional via C13s). C14 shipped the generic HashMap follow-up.

### P3 — Expand (after hello)

1. ~~Language surface through C20~~ (funs/lambdas, mutable class/Array/Fun capture MVP) → later: true borrow, Array-of-iface, safe live Array views
2. Runtime: ~~GC + process I/O + String Array free + Fun env RC~~ → later: channels/tasks; fix `Io.args` strdup vs free
3. Toolchain: ~~path deps + registry K1 offline~~ → ~~**S2:** verified HTTPS + nested locked registry deps~~ → later: K1b/K2 publish
4. Stdlib: ~~io + collections + C13 toString/case/eprint/tryWrite + C14 generic HashMap + C15 generic HashSet + C18 hash-collection HOFs + C19 accessors/entry snapshots/entry for-in + C20 snapshot iterators~~ → later: live iterator/entry-view APIs and entry mutation
5. Cross targets + signed releases — ~~**S2 contract:** Linux amd64, macOS arm64/amd64~~; Windows amd64 deferred → ~~**C13s** signing note~~ → later: minisign / notarization

S2 production toolchain implementation: [`docs/plans/2026-07-21-s2-production-toolchain.md`](plans/2026-07-21-s2-production-toolchain.md). Release publication remains pending.

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
