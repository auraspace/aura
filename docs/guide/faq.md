---
title: FAQ
section: Project
order: 75
summary: Common questions about install, null, errors, GC, and docs vs RFCs.
---

# FAQ

## Getting started

### Do I need to install Aura globally?

For alpha, install from a clone:

```bash
cargo install --path crates/aura-cli
aura new hello && aura run hello
```

Or run in-tree without installing: `cargo run -p aura-cli -- …`.  
Details: [Install](./install.md), freeze notes [0.1.0-alpha](/docs/releases/0.1.0-alpha) (repo path `docs/releases/0.1.0-alpha.md`).

### Why is a C compiler required?

`build` / `run` use a **C backend**: Aura → C → system `cc`, linked with `runtime/aura_rt.c`. LLVM is the longer-term backend ([RFC-004](/rfc/004)).

### Where are examples?

Under `corpus/`. Prefer corpus over stale snippets in chat.

## Language

### Is everything nullable like old Java?

No. `T` is non-null; `T?` is opt-in. See [Types & nullability](./types-and-nullability.md).

### Result or exceptions?

- **Expected** failure → `Result<T, E>`
- **Unexpected** → `throw` / `try` / `catch`

See [Control flow & errors](./control-flow-and-errors.md).

### Class `==` compares fields?

No — **identity** (reference). Compare fields explicitly if you need structural equality.

### Are tasks / async ready?

Vision and RFCs are Accepted, but full task runtime surface is still limited in code. Check the [roadmap map](./roadmap.md#rfc-accepted-vs-implemented).

## Toolchain

### Docs vs RFCs — which wins?

- **User docs** teach
- **RFCs** define

If they disagree, file an issue; RFCs are the design source of truth until docs catch up.

### Why is my `std.io` import failing?

Use package mode (`aura.toml` + directory target), ensure `std/` is present, and see [Standard library](./standard-library.md).

### Does `aura test` need a framework package?

MVP discovery runs `@test` functions via the CLI. `std.assert` helps with assertions. Broader framework design: [RFC-011](/rfc/011).

## Site & contributing

### How do I add a guide page?

1. Add `docs/guide/my-page.md` with frontmatter (`title`, `section`, `order`, `summary`)
2. Rebuild the site — it appears under `/docs`

### How do I propose a language change?

Start from `docs/rfc/TEMPLATE.md`, follow status rules, add corpus when behavior is user-visible. See [Contributing](./contributing.md).

## Next

- [Getting started](./getting-started.md)
- [Roadmap](./roadmap.md)
- [RFC catalog](/rfc)
