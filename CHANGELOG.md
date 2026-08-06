# Changelog

## 0.1.1-alpha.5 (2026-08-06)

Next alpha release

Full notes: [`docs/releases/0.1.1-alpha.5.md`](docs/releases/0.1.1-alpha.5.md).

### Changes

- fix(expr): update ownership logic for boxed String locals in string_expr_is_owned_temp
- feat(release): add manual release workflow for building and publishing GitHub releases
- fix(tests): update expected output for aura test to match actual results
- refactor(call): update method overload types to use ClassMethodSig for consistency refactor(emit): simplify json field name extraction logic and improve clarity
- refactor(cli): streamline error handling and improve progress reporting in check and build commands
- feat(example): add dotenv example with loader and configuration
- refactor: remove http-engine-aura example and add todo-app example
- refactor(example): adopt current aura oop APIs
- fix(codegen): upcast secondary constructor arguments
- refactor(example): organize http engine library modules
- docs(oop): mark overload resolution complete
- fix(oop): preserve nested call and static generic resolution
- docs(oop): document defaults varargs and overloads
- feat(oop): complete interface and constructor overloads
- feat(oop): support method overloads and variadic arrays
- feat(oop): support defaults and free-function overloads
- feat(oop): support secondary constructors
- fix(oop): substitute generic interface defaults
- feat(oop): support interface default methods
- feat(oop): add super method calls
- feat(oop): support abstract method declarations
- feat(oop): add companion object syntax
- docs: update testing instructions in README and contributing guide
- feat(http): Introduce Aura HTTP engine example with routing and middleware

## 0.1.1-alpha.4 (2026-08-04)

Expanded typed standard-library support, reflection, async ownership, and release targets.

Full notes: [`docs/releases/0.1.1-alpha.4.md`](docs/releases/0.1.1-alpha.4.md).

### Changes

- fix(lsp): simplify word occurrence handling by removing unnecessary references
- fix(crypto): update target attributes for SHA256 optimizations
- docs(std): align crypto and tls completion boundaries
- fix(codegen): register generic async return layouts
- fix(runtime): link zlib in native regression gates
- fix(runtime): link zlib in async ffi smoke
- feat(std.json): support recursively nested primitive arrays
- feat(std.json): decode nested primitive arrays
- feat(std.json): decode primitive generic arrays
- fix(std.io): add typed wrappers and preserve generic result ownership
- feat(std.http): add typed streaming outcomes
- feat(std.stream): add typed transport outcomes
- fix(std): fail closed remaining intrinsic fallback bodies
- refactor(std): name intrinsic runtime handles explicitly
- docs(std.reflect): document generic metadata
- feat(std.reflect): expose generic interface metadata
- feat(std.reflect): expose generic type metadata
- fix(std.multipart): preserve boundary-like body data
- fix(std.multipart): harden multipart parser and encoder
- fix(std): fail closed intrinsic fallback bodies
- fix(std.task): close generic join payloads
- fix(std.signal): fail closed outside intrinsic backend
- feat(generics): support multi-level nested substitution
- feat(json): decode struct arrays
- feat(json): decode nested struct fields
- feat(json): decode unit enum arrays
- docs(std): align JSON mapping contract
- test(std.test): verify snapshot persistence
- feat(json): decode unit enums in typed classes
- feat(json): decode class arrays in typed classes
- feat(json): decode primitive arrays in typed classes
- feat(test): report native benchmark durations
- feat(json): support recursive generic class decoding
- feat(json): decode nested class fields safely
- feat(test): add bounded benchmark discovery
- docs(std): align bounded task and json status
- docs(std): align reflection and protocol status
- fix(reflect): hide non-public members
- fix(runtime): install shutdown signals transactionally
- feat(test): implement deterministic advanced std.test hooks
- feat(reflect): enforce opt-in runtime metadata
- fix(runtime): make log level configuration thread-safe
- docs(std): replace alpha API lock with implementation status
- feat(crypto): add SHA-256 benchmark and tests for portable and accelerated implementations
- refactor: code structure for improved readability and maintainability
- feat(ci): add support for linux-arm64 in CI and release workflows
- feat(runtime): add precise typed GC tracing
- feat(codegen): recognize tier-two native hosts
- feat(lsp): expose structured completion suggestions
- feat(lsp): cancel in-flight stdio requests
- feat(lsp): preserve binding identities across edits
- feat(release): declare packaged target-neutral sysroot
- fix(lsp): scope references and rename by binding
- docs(debt): narrow API-003 boundary inventory
- fix(parser): preserve declarative macro invocation spans
- fix(analysis): bound query cache with LRU eviction
- docs(std): reconcile runtime-backed package contracts
- fix(parser): reject empty channel sends
- feat(std): complete TLS and typed error adapters
- feat(std-websocket): add bounded client framing
- feat(std-udp): add runtime-backed datagrams
- feat(std-json): decode bounded flat classes
- feat(std-collections): add list snapshots
- fix(codegen): substitute generic local types
- fix(codegen): preserve owned outcome call arguments
- feat(codegen): emit reflection member type metadata
- feat(std-collections): make List formally iterable
- feat(std-json): implement primitive JSON decode generics
- feat(codegen): emit reflection member names from class metadata
- feat(codegen): add compiler-backed reflection type metadata
- feat(std-compress): implement hex-safe compression intrinsics
- feat(std-multipart): implement bounded multipart parser and encoder
- Implement std.crypto baseline primitives
- Complete bounded JSON traversal ownership
- docs(std): sync alpha completion and remaining debt
- feat(std-test): execute core assertion and smoke helpers
- feat(std-json): implement bounded parse options and value metadata
- feat: enhance MIR rendering for assertions and add corresponding tests
- fix: inline async polling ownership
- docs: track generated std net accept gap

## 0.1.1-alpha.3 (2026-08-03)

Fix async GC ownership and release acceptance

Full notes: [`docs/releases/0.1.1-alpha.3.md`](docs/releases/0.1.1-alpha.3.md).

### Changes

- feat: enhance workspace tests to include ASan options for leak detection
- feat: enhance FFI and HTTP error handling, add payload reference tests
- feat: add interactive AuraCanvas background and code showcase section to home page

## 0.1.1-alpha.2 (2026-07-31)

Second alpha patch release with portability and registry acceptance fixes

Full notes: [`docs/releases/0.1.1-alpha.2.md`](docs/releases/0.1.1-alpha.2.md).

### Changes

- feat: dynamically set AURA_VERSION from Cargo.toml in install-smoke.sh and improve TAG_VERSION handling in package-release.sh
- feat: replace INADDR_LOOPBACK with AURA_NET_LOOPBACK_ADDR for consistency in loopback address handling
- feat: update registry acceptance script to use aura-package for testing and verification
- feat: enhance call expression emission and improve health check response validation in smoke test
- feat: add POSIX compliance flag to C compilation scripts for improved compatibility
- feat: update site metadata and enhance home page content for clarity and engagement
- feat: enhance documentation and standard library details
- feat: enhance standard library documentation and improve error handling
- style: format code for consistency and readability in health handler
- feat: update VSCode extension configuration and build process
- feat: enhance Aura Language Support with project commands and CLI integration
- feat: complete bounded async HTTP handler stack
- feat(sema): add return path analysis and diagnostics for non-unit functions
- feat(vscode): update .gitignore to include bin directory and add NOTE.md for language-server binaries
- feat: implement minimal aura.toml parsing and loading
- feat: add Aura Language Support extension for VS Code

## 0.1.1-alpha.1 (2026-07-29)

First alpha release

Full notes: [`docs/releases/0.1.1-alpha.1.md`](docs/releases/0.1.1-alpha.1.md).

### Changes

- refactor(server): improve string handling and simplify logic in completion and documentation functions
- fix(emit): replace aura_gc_alloc_full with malloc for async function memory allocation
- fix(emit): update memory allocation to use aura_gc_alloc_full for async function
- docs: enhance language tour with recommended learning path and topic guide updates
- refactor(docs): update syntax and examples for enums, results, and testing commands
- fix(sema): preserve package-qualified class lookup
- feat(collections): add live collection iterators
- feat: refactor collections to support key-based mutation handles and invalidation-checked live entry views
- feat: add tests and implementation for class String ownership in constructors
- feat: implement class inheritance, visibility, and constructor argument checks
- feat: complete Aura language server MVP
- feat(docs): add Async HTTP Handler Completion Plan
- feat(cli): complete aura formatter
- feat(fmt): add --check option for formatting verification
- perf: reuse parsed analysis query results
- feat: complete analysis snapshots and queries
- feat: add shared aura analysis facade
- feat(formatter): enhance formatting capabilities for `.aura` files and directories
- feat(avm, install): improve symlink handling for macOS in version management
- docs: sync release docs and clean alpha gates

## 0.1.1-alpha (2026-07-28)

Production-facing alpha release

Full notes: [`docs/releases/0.1.1-alpha.md`](docs/releases/0.1.1-alpha.md).

### Changes

- feat(package): update packaging script to use custom distribution directory feat(install): modify local package smoke mode to create temporary package directory feat(emit): adjust C emission for async function to improve structure
- feat(release): configure production environment for signing releases
- feat(audit): simplify incomplete contract row handling in alpha completion audit
- feat(validate): refactor duplicate ID check to improve validation logic
- feat(emit): enhance async function emission with foreign handle management and cleanup
- feat(tests): update file path formats in test cases to include '-data' suffix
- feat(file): improve file opening with retry on EINTR and add last error function
- feat(emit): enhance task management in bounded spawn pollers with ownership tracking
- feat(emit): enhance memory management for owned String and array types in async functions
- feat(call): enhance handling of owned temporary string arguments in call emission
- feat(call): enhance string handling in print functions for owned temporary expressions
- feat(ffi): enhance foreign handle null check in async function
- feat(http): manage async handle frame in HTTP connection
- feat(ffi): add handle free function for unowned opaque handles
- feat(release): implement bounded alpha release profile and update audit scripts
- test(async): cover string array match bindings
- feat(async): extend CFG for-in lowering
- feat(async): lower array for-in awaits in general CFG
- feat(ffi): transfer foreign handles through channels
- feat(http): expose typed borrowed accessors
- feat(async): lower range loops in general CFG
- feat(io): expose owned std.net connect handle
- feat(async): clone array match bindings across await
- feat(async): own string match bindings across await
- feat(async): support primitive match bindings across await
- test(io): cover caller-owned file task awaits
- test(io): cover regular file async readiness
- feat(async): lower match branches across await
- fix(ffi): retain foreign handles across cfg awaits
- fix(async): preserve aggregate moves across CFG awaits
- test(async): cover caller-owned string task CFG outcomes
- feat(async): retain handles across general CFG awaits
- feat(ffi): support nested opaque handle crossings
- feat(http): expose borrowed typed request response views
- test(async): cover loop branch CFG awaits
- feat(async): generalize CFG await shape detection
- feat(async): preserve heap classes across CFG awaits
- feat(async): extend general CFG awaits to Bool
- feat(async-io): retain handles across caller awaits
- feat(async): support aggregate arrays in CFG awaits
- feat(async): clone string arrays across general awaits
- feat(ffi): transfer owned handles through bounded tasks
- feat(http): pin typed handles across async handlers
- test(async-io): cover native file round trips
- feat(async): extend general CFG lowering to String
- feat(async): generalize CFG aggregate suspension
- feat(async-io): retain foreign handles across spawn
- feat(async-io): own typed file handles
- feat(async-io): lower typed TCP stream operations
- feat(async-io): lower typed AuraFile writes
- feat(async-io): lower typed AuraFile reads
- feat(async): preserve caller-owned task children
- feat(async): add explicit scalar CFG lowering
- feat(async): lower range-for awaits into task frames
- feat(async-io): lower descriptor writes into task frames
- feat(async-io): lower descriptor reads into task frames
- feat(async): expose typed class failure outcomes
- feat(async): reclaim continuation children
- feat(async): reclaim if assignment children
- feat(async): reclaim branch join children
- feat(async): reclaim conditional loop children
- feat(async): reclaim loop await children
- feat(async): reclaim multi-await child frames
- feat(runtime): reclaim pending task handles
- feat(async): own aggregate values across awaits
- feat(async): generalize conditional loop awaits
- feat(async): lower three conditional loop awaits
- feat(async): release terminal task handles by scope
- feat(async): preserve raw class error payloads
- feat(async): normalize class failures into owned outcomes
- test(async): cover nested suspended failures
- feat(async): resume conditional loop awaits
- feat(async): preserve arrays across loop awaits
- feat(async): own array branch join outcomes
- feat(async): own string branch join outcomes
- feat(async): lower loop branch await joins
- feat(async): resume branch joins through common continuation
- feat(async): await heap class child results
- feat(async): own heap class task outcomes
- feat(async): support typed array spawn outcomes

## 0.1.0-alpha (2026-07-21)

Release `0.1.0-alpha` is published with GitHub Release assets and the public
installer for the supported Unix targets.

Full notes: [`docs/releases/0.1.0-alpha.md`](docs/releases/0.1.0-alpha.md).

### Changes

- feat: implement standard library package resolution and enhance installer script
- docs: sync guide and release notes with 0.1.0-alpha
- feat: add Aura Version Manager (avm) and embed in installer script for improved version management
- feat: enhance cross-compilation support for macOS and improve release packaging script
- fix(ci): avoid GNU tar SIGPIPE in release smoke step
- feat: rename aura-switch to avm and update related documentation
- refactor: improve output formatting in release preparation script
- feat: add initial release notes for 0.1.0-alpha
- feat: C11d String.substring, notes dogfood, this.method fix
- feat(cli): C11c aura new/init/version package scaffold
- feat: C11a–b std.io file I/O and Fun capture-env free
- docs: sync C10j status across README, guide, corpus, and RFCs
- docs(C10j): close C10a–C10j batch on roadmap and debts
- feat(stdlib): C10i map_ints/filter_ints/fold_ints helpers
- feat(lang): C10h lambda val captures via fat-pointer Fun
- feat(lang): C10g lambda block body
- docs(C10): plan C10a–j and mark C10a–f done on roadmap
- feat(lang): C10c–f non-capturing lambdas and fun types
- feat(dx): C10b diagnostics context line and type-mismatch notes
- docs(C9j): close C9a–C9j batch on roadmap and debts
- feat(lang): C9i is type test
- feat(parser): C9h string interpolation via + desugar
- feat(lang): C9f type alias and C9g top-level const
- feat(parser): C9e expression-body functions
- feat(lang): C9d String + concatenation
- feat(lang): C9c Array.clone owning buffer copy
- feat(stdlib): C9b HashMap auto-resize on load
- feat(lang): C9a generic class implements mono
- docs(C8l): close C8c–C8l batch on roadmap and debts
- feat(cli): C8k aura.lock registry schema v0
- feat(codegen): C8j non-destructive Array field bind
- feat(stdlib): C8i HashMap String→Int open addressing
- feat(stdlib): C8h for-in over Map.keys and Set
- feat(stdlib): C8g generic Set<T>
- feat(codegen): C8f free nested Array element buffers
- feat(codegen): C8e nested Array<Array<T>> mono
- feat(stdlib): C8d Iterable<E> in std.collections + for-in
- feat(lang): C8c generic interface implements mono
- feat(cli): C8b path lock existence check and registry spike
- feat(stdlib): C8a generic Map<K,V> via Array type-param elems
- docs(C7j): defer Array element drop for MVP
- feat(lang): C7i generic interface type params foundation
- docs(C7h): reject Array of interface for MVP
- feat(sema): C7g multi-error collect in declaration phase
- feat(stdlib): C7f Map.remove and Map.clear
- feat(stdlib): C7e std.collections Set for String
- docs(C7d): plan and roadmap for C7a–C7j batch
- feat(codegen): C7c move-out Array field on return/bind/assign
- feat(runtime): C7b free and mark Array fields on GC objects
- feat(codegen): C7a Int?/Bool? tagged optional C emit
- feat(site): add mobile navigation menu to header
- docs(C6j): close C6a–C6j batch on roadmap and plan
- feat(compiler): C6g Array of enum by-value elements
- refactor: reorganize imports and improve code formatting across multiple files
- feat: enhance documentation and SEO metadata across site components
- feat(deploy): update Cloudflare Pages deployment process and README
- chore: add wrangler as a devDependency and update pnpm-lock.yaml
- feat(ci): upgrade Node.js version to 24 in CI workflows
- chore(ci): update pnpm action setup to use version from package.json
- chore(site): move aura-site into root pnpm monorepo workspace
- feat(sema): C6h multi-error collect in function bodies
- feat(runtime): C6e GC mark through Array-of-class buffers
- feat(codegen): C6i Array ownership moves into class fields
- ci(site): use GitHub environment static-pages for Cloudflare deploy
- ci(site): deploy to Cloudflare Pages at aura.fadosoft.com
- feat(stdlib): C6f Map String→Int and Array-as-field codegen
- feat(codegen): C6d Array call/return bindings own buffers
- feat(compiler): C6a–C6c deep GC mark, Array param move, iface Iterable
- refactor(docs): remove outdated plans and specifications for C4 series and static site design
- ci: add PR/main workflow for compiler and site
- fix(lockfiles): update aura.lock comments for clarity and add nested_mid.lock
- feat(site): ship user docs with Shiki, search, and richer guides
- feat(site): warm editorial homepage with Motion and Tabler icons
- feat(site): Tailwind v4 and feature-based /rfc layout
- fix(site): publish RFC catalog at /rfc with working SSG
- docs: C5n mark C5e–C5n plan complete
- feat(compiler): C5l–C5m gc_collect and Array shallow docs
- feat(sema): C5k expected/found type mismatch messages
- feat(compiler): C5h–C5j String startsWith, contains, endsWith
- feat(codegen): C5g register heap-class locals as GC roots
