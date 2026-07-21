# Changelog

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
