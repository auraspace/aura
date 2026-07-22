# RFC-001: Language Specification

| Field        | Value                                                         |
| ------------ | ------------------------------------------------------------- |
| **RFC**      | 001                                                           |
| **Title**    | Language Specification                                        |
| **Status**   | Accepted                                                      |
| **Layer**    | Language                                                      |
| **Authors**  |                                                               |
| **Created**  | 2026-07-15                                                    |
| **Updated**  | 2026-07-16                                                    |
| **Estimate** | 80–120 pages                                                  |
| **Depends**  | RFC-000                                                       |
| **Blocks**   | RFC-002, RFC-003, RFC-004, RFC-006, RFC-007, RFC-009, RFC-010 |

---

## 1. Abstract

This RFC defines the **surface syntax and core language semantics** of Aura: lexical structure, grammar sketch, declarations (classes, interfaces, functions, packages), expressions and statements, modules/visibility, and the user-visible shape of nullability, errors, async, and attributes.

Deep type rules live in **RFC-002**; memory, tasks, and races in **RFC-003**; attributes/metadata detail in **RFC-009**; macros in **RFC-010**. Syntax here is **pseudo-Aura** until a frozen grammar file lands in-repo.

## 2. Motivation

### 2.1 Problem statement

Without a single surface specification, compiler, formatter, and docs diverge. Aura needs a **statement-oriented, Java-like** surface that still supports modern expression forms, null-safe types, `Result`, exceptions, and task-based async—without requiring ownership annotations.

### 2.2 Why now

Wave-1 foundation: every later RFC quotes keywords, declaration forms, and module rules from here.

### 2.3 Success metrics

- Unambiguous programs can be written and type-checked (with RFC-002).
- Grammar parseable by a standard approach (hand-written recursive descent first; tree-sitter grammar as companion).
- A short “language tour” compiles once the compiler MVP exists.

## 3. Goals

- Formal-ish specification: lexical rules + grammar sketch + semantics prose.
- Deterministic evaluation order where it matters (argument order, field init).
- Stable surface for compiler, formatter, and macros.
- Align with RFC-000 locked decisions (classes, `T?`, exceptions + `Result`, tasks).

## 4. Non-goals

- Full formal operational semantics (optional later appendix).
- Stdlib API (→ RFC-007).
- Detailed type inference algorithm (→ RFC-002).
- Memory/concurrency operational model (→ RFC-003).
- Complete EBNF frozen file in this draft (tracked as deliverable).

## 5. Prior art & alternatives

| Approach      | Notes                                            | Influence                        |
| ------------- | ------------------------------------------------ | -------------------------------- |
| Java / Kotlin | Classes, methods, packages, nullability (Kotlin) | Primary surface                  |
| C#            | async, attributes                                | Secondary                        |
| Go            | package model simplicity                         | Packages/tasks spirit only       |
| TypeScript    | DX ergonomics                                    | Not structural typing default    |
| Rust          | Expression-oriented blocks                       | Selective (`if`/`match` as expr) |

## 6. Design

### 6.0 MVP surface (compiler C0–C1)

This subsection freezes the **subset** that the first compiler milestones must implement. Full v1 surface remains in later subsections; anything not listed here was **out of scope for C0/C1** unless an RFC amend expands this table.

**Toolchain progress (2026-07-22):** the Rust compiler has shipped through **C21i** plus **S2** production-toolchain work (see [roadmap](../roadmap.md)): classes, interfaces, generics+bounds, `struct`/`enum`/`match`, exceptions, packages/`import`/`aura.lock`, `Array`/`for`/`?.`/`?:`, scoped non-owning `ref T` with lexical escape checks, borrow-safe Array field returns, GC mark/sweep, `std.io` (console/file/argv/stdin/exit and Result wrappers), `assert`, generic collections, deterministic read-only collection snapshots/iterators, first-class funs/lambdas with value captures and mutable `var` class/Array/Fun capture MVPs, String tools, registry dependency consumption, and C21 formatter/diagnostic/test-report tooling. Async/tasks, macros, mutable/nullable/nested borrows, `Array<Interface>`, live collection views, and mutation-through-entry remain deferred. §6.0 remains the historical C0/C1 freeze; later milestones are tracked in the roadmap, not by rewriting this freeze.

**Milestones** (aligned with RFC-004 §11 and this RFC §11):

| Milestone             | Compiler goal                                                          | Language surface                                       |
| --------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------ |
| **C0**                | `aura check` — lex, parse, basic name checks                           | §6.0.1–6.0.3                                           |
| **C1**                | `aura build` — native hello (interim **C backend** + `cc`; LLVM later) | C0 + print/runtime hooks                               |
| **C1b**               | Simple classes + methods                                               | + §6.0.4 (**implemented**)                             |
| **Post-C1 (C2–C12+)** | Generics, packages, Array, GC, lambdas, process I/O, …                 | Shipped slices in roadmap; async/macros still deferred |

#### 6.0.1 Lexical (C0)

| Item                        | MVP rule                                                                                                                                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Encoding                    | UTF-8                                                                                                                                                                                                        |
| Comments                    | `//` line; `/* */` non-nesting block                                                                                                                                                                         |
| Identifiers                 | ASCII `[A-Za-z_][A-Za-z0-9_]*` (Unicode XID later)                                                                                                                                                           |
| Keywords (hard)             | `package`, `import`, `as`, `class`, `fun`, `val`, `var`, `if`, `else`, `while`, `return`, `true`, `false`, `null`, `pub`                                                                                     |
| Soft / deferred (C0 freeze) | Originally deferred: `match`, `async`, `await`, `spawn`, `interface`, `enum`, `struct`, `for`, `in`, … — many are now **implemented** post-C1 (see roadmap); still deferred: `async`/`await`/`spawn`, macros |
| Literals                    | decimal `Int`, `true`/`false`, `"..."` strings (no interpolation in C0), `null`                                                                                                                              |
| Operators                   | `+ - * / %`, `== != < <= > >=`, `&& \|\| !`, `=`, `.`, `( ) { } , : ?`                                                                                                                                       |
| Semicolons                  | optional; newline/brace terminated                                                                                                                                                                           |

#### 6.0.2 Grammar (C0)

```ebnf
File        = "package" Path Decl*
Path        = Ident ("." Ident)*
Decl        = FunDecl
FunDecl     = "fun" Ident "(" Params? ")" (":" Type)? Block
Params      = Param ("," Param)*
Param       = Ident ":" Type
Type        = Ident "?"?                 (* nominal + optional nullability *)
Block       = "{" Stmt* "}"
Stmt        = VarStmt | IfStmt | WhileStmt | ReturnStmt | ExprStmt
VarStmt     = ("val" | "var") Ident (":" Type)? "=" Expr
IfStmt      = "if" "(" Expr ")" Block ("else" Block)?
WhileStmt   = "while" "(" Expr ")" Block
ReturnStmt  = "return" Expr?
ExprStmt    = Expr
Expr        = ... Pratt: call, member, unary, binary, primary
Primary     = Ident | Literal | "(" Expr ")"
```

- One package declaration per file; **no** multi-file packages in C0.
- Top-level **functions only** (no `class` until C1b).
- Entry: `fun main()` (optional `: Unit` / no return type).

#### 6.0.3 Semantics (C0 check / C1 run)

| Topic            | MVP                                                                                 |
| ---------------- | ----------------------------------------------------------------------------------- |
| Types            | `Int`, `Bool`, `String`, `Unit`; user types deferred; `T?` parse + local flow later |
| Name resolution  | Single file: funs + locals; calls to unknown names → error                          |
| Control flow     | `if` / `while` / `return`                                                           |
| Runtime (C1)     | Linked stub: `println(String)` (or intrinsic) + process exit                        |
| Concurrency / GC | **Declared** by RFC-000/003; **not** implemented in C0/C1 (single-threaded stub OK) |

#### 6.0.4 Classes (C1b only)

```aura
class Greeter(val name: String) {
  fun greet(): String {
    return "Hello, " // + name in later string ops
  }
}
```

- Final classes only; primary constructor fields; instance methods; `this` optional later.
- No inheritance, interfaces, generics, or `companion` in C1b.

#### 6.0.5 Explicitly deferred (post-C1)

**Originally deferred at C0/C1 freeze** (many now shipped — see roadmap C2–C10j):

| Item                                                            | Toolchain status (2026-07-20)                                                                                                                  |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Generics, interfaces, `struct`/`enum`/`match`                   | **Implemented** (C2–C3)                                                                                                                        |
| Exceptions / `throw`/`try`/`catch`/`finally`                    | **Implemented** (C3c/C3g)                                                                                                                      |
| Multi-file packages, `import`, path deps, `aura.lock`           | **Implemented** (C3e–C3p, C4j); lock schema v0 (C8k)                                                                                           |
| `for` ranges / for-in (Array, String bytes), `break`/`continue` | **Implemented** (C3h–C3l, C3w)                                                                                                                 |
| Builtin `Array<T>`, null `?:` / `?.`, `if` expr                 | **Implemented** (C3j+, C4m/C4s/C4t)                                                                                                            |
| String `+` / interpolation `${}` (idents), `type`/`const`, `is` | **Implemented** (C9d–C9i)                                                                                                                      |
| Lambdas / fun types `(T) -> U`                                  | **Implemented** (C10c–j + C12k–m + C20c–e captures); escaping live Array ownership still deferred                                              |
| `async`/`await`/`spawn`                                         | **C22 surface and checks landed; lowering is partial** — no-await tasks/empty spawn only; await state machines and non-empty captures deferred |
| Attributes/macros, full Unicode identifiers                     | **Still deferred**                                                                                                                             |
| Iterable protocol                                               | **Implemented** (C8d); generic class implements C9a                                                                                            |
| Registry/semver fetch                                           | **Still deferred** (debts)                                                                                                                     |

**Corpus:** programs under `corpus/` exercise the implemented surface above; see [`corpus/README.md`](../../corpus/README.md).

### 6.1 Overview

Aura is a **multi-paradigm** language with a **class-based OOP** core:

| Axis          | Choice                                                                       |
| ------------- | ---------------------------------------------------------------------------- |
| Compilation   | Ahead-of-time to native (LLVM)                                               |
| Execution     | Linked runtime: GC + task scheduler                                          |
| Orientation   | **Statements** primary; **blocks / if / match** may yield values             |
| Nominal types | Classes, interfaces, enums; value `struct` (distinct from `class`)           |
| Modules       | Package-based; files belong to a package                                     |
| Entry         | `fun main(...)` in a designated root (convention: package main or `[[bin]]`) |

```text
Source file
  → package decl
  → imports
  → top-level decls (class, interface, enum, struct, fun, const, type alias)
```

### 6.2 Lexical structure

**Character set & encoding:** UTF-8 source. Identifiers: Unicode XID rules (exact table TBD); ASCII letters/digits/`_` required for keywords.

**Keywords (reserved):**  
`package`, `import`, `as`, `class`, `interface`, `enum`, `struct`, `fun`, `val`, `var`, `const`, `if`, `else`, `match`, `case`, `while`, `for`, `in`, `break`, `continue`, `return`, `throw`, `try`, `catch`, `finally`, `async`, `await`, `spawn`, `is`, `as` (cast form disambiguated), `null`, `true`, `false`, `this`, `super`, `new` (optional sugar—prefer constructor call `Type(...)`), `pub`, `protected`, `private`, `static`, `abstract`, `override`, `open`, `final`, `where`, `type`, `unsafe`.

**Soft keywords** (contextual): `get`, `set`, `field`, `init`, `from` — reserved only in specific positions.

**Literals:**

| Kind       | Examples                                   |
| ---------- | ------------------------------------------ |
| Int        | `0`, `42`, `0xFF`, `1_000`                 |
| Float      | `3.14`, `1e-3`                             |
| Bool       | `true`, `false`                            |
| String     | `"hello"`, interpolation `"hi ${name}"`    |
| Raw string | `#"path\no-escape"#` (delimiter style TBD) |
| Char       | `'a'`                                      |
| Null       | `null` (only for `T?`)                     |

**Comments:** `//` line, `/* */` block (**non-nesting** v1). Doc comments: `///` or `/** */` attached to following decl.

**Semicolons:** Statements **do not require** trailing `;` (newline/brace terminated). `;` optional separator for multiple statements on one line. No significant indentation.

**Operators:** Arithmetic, comparison, logical, `?.` safe call, `?:` null-coalesce, `!!` unchecked non-null assert (discouraged; lintable), `->` in function types, `=>` in match/lambda if needed.

### 6.3 Grammar sketch

Source of truth will be `grammar/aura.ebnf` (future). Sketch:

```ebnf
File        = PackageDecl Import* Decl*
PackageDecl = "package" Path
Import      = "import" Path ("as" Ident)?
Decl        = ClassDecl | InterfaceDecl | EnumDecl | StructDecl
            | FunDecl | AsyncFunDecl | ConstDecl | TypeAlias | Attribute* Decl

ClassDecl   = Attr* Modifiers "class" Ident TypeParams?
              (":" TypeList)? ClassBody
FunDecl     = Attr* Modifiers "fun" Ident TypeParams? "(" Params? ")"
              (":" Type)? BlockOrExprBody
AsyncFunDecl = Attr* Modifiers "async" "fun" Ident TypeParams? "(" Params? ")"
              (":" Type)? BlockOrExprBody
AsyncExpr   = AwaitExpr | SpawnExpr | JoinExpr | CancelExpr
AwaitExpr   = "await" Expr
SpawnExpr   = "spawn" (Block | Expr)
JoinExpr    = "join" "(" Expr ")"
CancelExpr  = "cancel" "(" Expr ")"
```

Parser strategy: **hand-written recursive descent** with Pratt parser for expressions (RFC-004).

#### 6.3.1 C22 async/task syntax contract

C22 reserves `async`, `await`, `spawn`, `join`, and `cancel` as keywords. `async`
may prefix only a function declaration; `await` may occur only inside an
`async fun`; `join` and `cancel` take exactly one task-handle expression.
`spawn { ... }` is the canonical form. A bare `spawn` expression is accepted
only when its operand is a callable task body, as defined by RFC-003.

Valid examples:

```aura
async fun load(id: Int): User {
  return await fetchUser(id)
}

fun main() {
  val task = spawn { load(42) }
  val user = join(task)
  cancel(task)
}
```

Invalid forms:

```aura
fun main() { await load(42) }       // await outside async fun
async class Worker {}               // async cannot modify a class
fun main() { join() }               // missing handle
fun main() { cancel(a, b) }         // too many handles
```

Span behavior is part of the frontend contract. A valid node spans its whole
operator and operand, or the complete `async fun` declaration. Invalid
placement is anchored to the operator keyword; an invalid `async` modifier is
anchored to `async`. Missing operands or delimiters use the zero-width
insertion point where the token was expected. Extra operands are anchored to
the first unexpected token, and nested operand errors retain their inner span.
Recovery must not widen an operator span to the enclosing block.

### 6.4 Types (surface syntax)

Semantic detail → RFC-002. Surface only:

| Category          | Syntax examples                                                                       |
| ----------------- | ------------------------------------------------------------------------------------- |
| Primitives        | `Int`, `Long`, `Float`, `Double`, `Bool`, `Char`, `Byte`, `String`, `Unit`, `Nothing` |
| Class / interface | `User`, `List<String>`                                                                |
| Nullable          | `String?`, `User?`                                                                    |
| Arrays            | `Array<T>` (not `T[]`)                                                                |
| Tuples            | `(Int, String)`                                                                       |
| Function types    | `(Int, String) -> Bool`                                                               |
| Result            | `Result<T, E>` (stdlib / prelude)                                                     |
| Type alias        | `type UserId = Long`                                                                  |
| Generics          | `class Box<T>`, constraints via `where` / bound syntax                                |

**No raw nullable without `?`.** `null` has type `Nothing?` / bottom-nullable and only assigns to `T?`.

### 6.5 Declarations

#### Variables

| Form               | Meaning                                   |
| ------------------ | ----------------------------------------- |
| `val x: T = ...`   | Immutable binding                         |
| `var x: T = ...`   | Mutable binding                           |
| `const X: T = ...` | Compile-time constant (top-level / class) |

Type annotation optional when inferable (RFC-002).

#### Functions

```aura
fun add(a: Int, b: Int): Int {
  return a + b
}

// Expression body
fun double(x: Int): Int = x * 2
```

- Named functions; default args **yes** (resolved at call site).
- Rest/`vararg`: **yes** as `vararg xs: T`.
- Overloads: **yes**, resolved per RFC-002.
- Generics on functions: yes.

#### Classes

```aura
open class Animal(val name: String) {
  open fun speak(): String { return "..." }
}

class Dog(name: String, val breed: String) : Animal(name) {
  override fun speak(): String { return "woof" }
}
```

- Primary constructor in header (Kotlin-like) plus optional secondary `constructor` blocks.
- Single class inheritance; multiple **interfaces**.
- `open` / `final` / `abstract`: **classes are final by default**; mark `open` to allow subclassing.
- Nested `companion` object for type-level / “static” members (primary model; no free-floating Java `static` as the main surface).

#### Struct (value type — included in v1)

```aura
struct Point(val x: Int, val y: Int)
```

Semantics (copy vs ref) → RFC-002/003. Surface: no identity, no subclassing.

#### Enums

```aura
enum Color { Red, Green, Blue }

enum Shape {
  case Circle(radius: Double)
  case Rect(w: Double, h: Double)
}
```

#### Interfaces

```aura
interface Drawable {
  fun draw()
  fun bounds(): Rect { /* default method optional */ }
}
```

#### Arrays of interfaces (C20h spike)

`Array<I>` is a valid future surface type, but remains deferred for the C20
MVP. The C backend needs every Array element to have one stable size and one
well-defined ownership action. Two layouts were considered:

- A **fat element** stores an interface data pointer plus a method-table (or
  type) pointer inline. This gives direct dispatch and avoids one allocation
  per element, but makes Array copies, GC scanning, and element drops depend on
  interface-specific metadata. It also requires a stable ABI for nullable and
  value-backed implementations.
- A **boxed element** stores one GC-managed pointer per element. The box owns
  the concrete payload and carries its interface dispatch metadata. Array
  layout stays pointer-sized and simple, but every insertion may allocate,
  iteration adds indirection, and drop/GC must coordinate box finalization
  without double-releasing a payload.

The spike recommendation is to **defer both layouts** until borrow/lifetime
rules and a precise erased-value drop contract exist. If implementation is
reopened, boxed elements are the safer first C backend target because their
uniform pointer representation fits the current GC model; a fat layout should
only be reconsidered when allocation overhead is demonstrated to matter.
See the [C20h layout spike](../plans/2026-07-22-c20h-array-interface-spike.md)
for the comparison and follow-up conditions.

#### Modules, packages, visibility

| Modifier    | Meaning                                                       |
| ----------- | ------------------------------------------------------------- |
| (default)   | Package-private                                               |
| `pub`       | Public API                                                    |
| `protected` | Class + subclasses                                            |
| `private`   | Enclosing class / file (file-private for top-level `private`) |

#### Imports

```aura
import std.io
import std.collections.List as StdList
```

Re-exports: `pub import ...` (optional v1).

### 6.6 Expressions & statements

**Statements:** declarations, assignments, loops, `return`/`break`/`continue`/`throw`, expression statements.

**Expression forms:** literals, calls, field/method access, operators, lambdas, `if`/`match` expressions, blocks `{ ... }` (value = last expression if used as expr).

**Control flow:**

```aura
if (cond) { ... } else { ... }

val msg = if (ok) "yes" else "no"

match (shape) {
  case Circle(r) => ...
  case Rect(w, h) => ...
}

while (cond) { ... }
for (item in iterable) { ... }
for (i in 0..n) { ... }  // range syntax TBD
```

**Pattern matching:** destructure enums, sealed hierarchies, basics on tuples; exhaustiveness → RFC-002.

**Casts:**

- Smart cast after `is` check (flow-sensitive).
- Explicit: `x as Type` (checked), `x as? Type` → `Type?`, `x as! Type` unsafe assert.

**Error handling surface:**

```aura
try {
  risky()
} catch (e: IoError) {
  ...
} finally {
  ...
}

throw AppError("boom")

fun readConfig(): Result<Config, ConfigError> { ... }

match readConfig() {
  case Ok(c) => use(c)
  case Err(e) => log(e)
}
```

- **Exceptions:** unchecked hierarchy rooted at `Error` / `Throwable` (names TBD).
- **`Result`:** for expected failures; not forced by the type system on all functions.

**Async surface:**

```aura
async fun fetch(url: String): Bytes { ... }

fun main() {
  spawn { worker() }
  val body = await fetch("https://example.com")
}
```

Semantics → RFC-003. Keywords only here.

**Null surface:**

```aura
val s: String? = maybe()
val n = s?.length()
val t = s ?: "default"
val u = s!!   // assert non-null; panic/throw if null
```

### 6.7 Functions & calling convention (language level)

- Parameters are **references to GC objects** for class types; **by-value** for primitives and structs (details RFC-003).
- `this` receiver for instance methods; `super` for parent.
- First-class functions and lambdas: **`(x: Int) => x + 1`** or block body `(x: Int) => { ... }`.
- Closures: capture `val` by value/shared immutability; capture `var` through shared mutable storage. C20c–e cover class, Array, and nested Fun MVP lowering; Array owner movement and live-view safety still require a borrow contract. Exact lowering is tracked in RFC-004/toolchain notes.
- Generators/iterators: `Iterable` / `Iterator` protocols in stdlib; `yield` **not required v1**.

### 6.8 Modules & compilation units

- **Package** = namespace + visibility boundary. Directory layout convention: `src/<package/path>/File.aura` (exact layout RFC-008).
- **Compilation unit** = package graph of a target (bin/lib).
- **Cyclic packages:** forbidden across packages; cycles within a package allowed with restrictions (forward refs).
- Root manifest: `aura.toml` (RFC-005) lists packages/targets.

### 6.9 Attributes / decorators / annotations

```aura
@test
@derive(Debug, Equals)
@inline
class Foo { }
```

Retention and meaning → RFC-009. Macro attributes → RFC-010.

### 6.10 Unsafe / low-level surface

```aura
unsafe {
  // raw pointers, unchecked casts, FFI helpers
}
```

- `unsafe` blocks/functions required for raw memory and most FFI.
- Safe Aura must not observe undefined behavior from other safe Aura (GC + type system); FFI breaks the seal (RFC-003/006).

### 6.11 Examples

```aura
package hello

import std.io.println

class Greeter(val name: String) {
  fun greet(whom: String?): String {
    val target = whom ?: "world"
    return "Hello, ${target}, from ${this.name}"
  }
}

fun main() {
  val g = Greeter("Aura")
  println(g.greet(null))
}
```

```aura
package demo.http

interface Handler {
  fun handle(req: Request): Result<Response, HttpError>
}

class EchoHandler : Handler {
  override fun handle(req: Request): Result<Response, HttpError> {
    return Result.ok(Response(200, req.body))
  }
}
```

### 6.12 Error model / edge cases

| Topic            | Rule                                                                                                                                                                                                                                                                  |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ambiguous parses | Grammar prefers longest match; reserved keywords never identifiers                                                                                                                                                                                                    |
| Soft keywords    | Allowed as identifiers outside their context                                                                                                                                                                                                                          |
| Evaluation order | Left-to-right for arguments and operands unless specified                                                                                                                                                                                                             |
| Overflow         | Fixed-width ints: **checked in `dev`** (trap/throw on overflow); explicit wrapping operators (`&+` style or `wrappingAdd`) always available; `release` may omit checks for speed when using wrapping ops—default arithmetic policy documented with toolchain profiles |
| String interp    | Only expressions; no statement injection                                                                                                                                                                                                                              |

### 6.13 Compatibility & migration

- Language version / edition field in `aura.toml`.
- Deprecation: `@deprecated` attribute + compiler warnings.
- No promise of source compatibility with Java/Kotlin; “Java-like” is conceptual.

## 7. Open questions

| #   | Question                              | Options                    | Owner | Status                                                         |
| --- | ------------------------------------- | -------------------------- | ----- | -------------------------------------------------------------- |
| 1   | Classes final by default?             | yes                        | Lang  | **Resolved** — final by default                                |
| 2   | Companion vs static keyword           | companion                  | Lang  | **Resolved** — companion                                       |
| 3   | Array syntax `Array<T>` vs `T[]`      | `Array<T>`                 | Lang  | **Resolved**                                                   |
| 4   | Lambda syntax                         | `(…) =>`                   | Lang  | **Resolved**                                                   |
| 5   | Checked exceptions                    | no                         | Lang  | **Resolved** — unchecked only                                  |
| 6   | Integer overflow policy               | checked dev + wrapping ops | Lang  | **Resolved** (release elision details open with profiles)      |
| 7   | Exact keyword set / soft-keyword list | §6.0.1 hard keywords v0    | Lang  | **Resolved** for C0 — expand as surface grows                  |
| 8   | Range syntax `0..n`                   | inclusive/exclusive        | Lang  | **Resolved** — exclusive `a..b` (C3h), inclusive `a..=b` (C3l) |

## 8. Rationale & trade-offs

Java-like classes maximize familiarity for service engineers. Kotlin-inspired nullability and primary constructors reduce boilerplate without adopting full Kotlin semantics. Statement orientation keeps control flow obvious; expression-`if`/`match` covers the useful 20% of expression-oriented style. Rejecting ownership keeps the surface aligned with GC (RFC-000). `Result` + exceptions avoid the false dichotomy of “all errors are exceptions” vs “all errors are values.”

## 9. Unresolved / future work

- Full formal grammar file in repo
- Tree-sitter grammar
- Spec test suite (syntax corpus)
- Exact prelude names (`Unit`, `Nothing`, `Result`)

## 10. Security & safety considerations

- No `eval` of source strings in language core.
- String interpolation does not execute code beyond expression evaluation.
- `unsafe` and `!!` are lint targets; security-sensitive codebases can forbid them.
- Macro expansion hygiene (RFC-010) prevents accidental identifier capture.

## 11. Implementation plan (optional)

| Phase | Scope                     | Exit criteria          |
| ----- | ------------------------- | ---------------------- |
| G0    | Lexer + subset grammar    | Parse hello            |
| G1    | Core decls + control flow | Spec examples parse    |
| G2    | Full surface v1           | Grammar frozen for MVP |

## 12. References

- RFC-000, RFC-002, RFC-003, RFC-009, RFC-010
- Kotlin language reference (nullability, primary constructors)
- Java Language Specification (classes, overloading concepts)
- Go language spec (packages—contrast only)

---

## Changelog

| Date       | Author | Change                                                                                            |
| ---------- | ------ | ------------------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Status → **Accepted** — Review: MVP surface frozen, open Qs resolved, C0–C4t implement against it |
| 2026-07-16 |        | Sync §6.0 notes with C4t toolchain; resolve range `..` / `..=` (Q8)                               |
| 2026-07-15 |        | Add §6.0 MVP surface for compiler C0–C1; resolve keywords v0                                      |
| 2026-07-15 |        | Initial skeleton                                                                                  |
| 2026-07-15 |        | Solid draft: Java-like surface, nullability, Result, tasks keywords                               |
| 2026-07-15 |        | Lock lean surface decisions (final, companion, Array, lambda, …)                                  |
