---
title: FAQ
section: Project
order: 75
summary: Common questions about install, null, errors, GC, and docs vs RFCs.
---

# FAQ

## Getting started

### Do I need to install Aura globally?

For **0.1.0-alpha**, prefer the installer (versioned under `~/.aura`):

```bash
curl -fsSL https://aura.fadosoft.com/install.sh | bash
export PATH="$HOME/.aura/bin:$HOME/.local/bin:$PATH"
aura new hello && aura run hello
```

Contributors can still `cargo install --path crates/aura-cli` from a clone, or run in-tree: `cargo run -p aura-cli -- …`.

Details: [Install](./install.md). Freeze notes: [0.1.0-alpha on GitHub](https://github.com/auraspace/aura/blob/main/docs/releases/0.1.0-alpha.md).

### Why is a C compiler required?

`build` / `run` use a **C backend**: Aura → C → system `cc`, linked with `aura_rt.c`. LLVM is the longer-term backend ([RFC-004](/rfc/004)).

### Where are examples?

Under `corpus/` in the monorepo, plus `examples/notes`. Prefer corpus over stale snippets in chat.

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

Use package mode (`aura.toml` + directory target), ensure `std/` is present (or auto-prelude resolves), and see [Standard library](./standard-library.md).

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
