---
title: Contributing
section: Project
order: 80
summary: How to work on Aura — RFCs, corpus, compiler, and site.
---

# Contributing

Aura is developed in the open under the **MIT** license.

## Where to contribute

| Area                | Path                | Notes                                              |
| ------------------- | ------------------- | -------------------------------------------------- |
| Language / design   | `docs/rfc/`         | Copy `TEMPLATE.md`; statuses are controlled        |
| Compiler / CLI      | `crates/`           | Rust workspace; tests via `cargo test --workspace` |
| Runtime             | `runtime/aura_rt.c` | Linked into native builds                          |
| Std packages        | `std/`              | `io`, `assert`, …                                  |
| Executable examples | `corpus/`           | Preferred proof for features                       |
| User docs           | `docs/guide/`       | This site’s `/docs` content                        |
| Website             | `site/`             | Vite + React; `pnpm site:dev`                      |

## Design process

1. **RFC** for non-trivial language or toolchain changes
2. **Corpus** sample when behavior is user-visible
3. **Compiler / runtime** implementation with tests
4. **User guide** update when the feature is teachable

Read [RFC-000](/rfc/000) for principles. Use the [RFC catalog](/rfc) and [dependency graph](/rfc/graph) to see how documents block each other.

## Local checks

```bash
cargo test --workspace
cargo run -p aura-cli -- check corpus/hello/main.aura
pnpm install          # once, from repo root (site workspace package)
pnpm site:test
pnpm site:build
```

CI (`.github/workflows/ci.yml`) runs the same gates on every PR and push to `main`:
workspace tests, corpus `check` (excluding `corpus/diag/`), a few `run`/`test` smokes, plus site test + build.

Site production deploys via `.github/workflows/deploy-site.yml` to **Cloudflare Pages** (`https://aura.fadosoft.com`). See [`site/README.md`](https://github.com/auraspace/aura/blob/main/site/README.md).

## Communication

- Issues and PRs on [GitHub](https://github.com/auraspace/aura)
- Keep user docs in English; identifiers match code

## Next

- [Getting started](./getting-started.md)
- [RFC catalog](/rfc)
