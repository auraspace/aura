---
name: aura-dev-guardrails
description: Enforce design guardrails while developing the Aura language/compiler in this repo. Use when adding or modifying Aura syntax/semantics, compiler pipeline stages, runtime ABI/behavior (alloc/strings/arrays/exceptions), repo layout, or target/backend support. Treat docs/ARCHITECTURE.md, docs/FOLDER_STRUCTURE.md, and docs/SYNTAX_DESIGN.md as the source-of-truth contract and update them in the same change whenever behavior or structure deviates.
---

# Aura Dev Guardrails

## Overview

Keep Aura implementation changes consistent with the project design docs. This skill provides a lightweight workflow to (1) classify a change, (2) re-load the relevant design constraints, (3) implement without breaking invariants, and (4) update docs when the design changes.

## Workflow

### 1) Identify change type (pick all that apply)

- **Syntax**: keywords, grammar, types, classes/interfaces, exceptions
- **Compiler frontend**: lexer/parser/AST, imports, resolver, type checker
- **IR/lowering**: HIR/MIR shape, passes, semantics lowering rules
- **Backend/linking**: codegen backend choice, object format, linker strategy
- **Runtime**: object layout, dispatch, memory management, exceptions/unwinding, C ABI
- **Repo layout**: new crates, new top-level dirs, multi-target structure

If a change touches more than one area, apply the strictest constraints across them.

### 2) Load the contract docs (must-do)

Before changing code or adding new folders/crates, read:

- `docs/ARCHITECTURE.md`
- `docs/FOLDER_STRUCTURE.md`
- `docs/SYNTAX_DESIGN.md`

Treat these docs as non-negotiable unless you are explicitly changing the design (in which case: update the docs in the same change).

### 3) Declare invariants + impacted sections (write before you implement)

In your implementation notes (or PR description), capture:

- **What is changing** (one paragraph)
- **Which doc(s) are impacted** (Architecture vs Folder Structure vs Syntax)
- **What stays the same** (explicit invariants)
- **What must be updated** (docs, tests, fixtures)

### 4) Implementation guardrails (hard rules)

- **If you change syntax**: update `docs/SYNTAX_DESIGN.md` in the same diff (keywords, examples, semantics).
- **If you change compiler stages/IR or runtime responsibilities**: update `docs/ARCHITECTURE.md`.
- **If you add/move crates or top-level directories**: update `docs/FOLDER_STRUCTURE.md`.
- **Avoid "silent divergence"**: do not implement behavior that contradicts the docs without updating them.

### 5) Exception feature guardrails (when touching throw/try/catch/finally)

- Keep semantics aligned with `docs/SYNTAX_DESIGN.md` and the runtime unwind plan in `docs/ARCHITECTURE.md`.
- Do not introduce exception interop across C boundaries in MVP without a deliberate doc update.

### 6) Local verification

If the repo has a runtime/compiler yet, run the project test/build commands. If not, at least run the guardrails checks:

```bash
bash codex/skills/aura-dev-guardrails/scripts/check_guardrails.sh
```

## Resources (optional)

### scripts/
`scripts/check_guardrails.sh` performs quick checks that the contract docs exist and haven't regressed on key decisions (for example: syntax uses `function`, not `fn`).

### references/
See `references/guardrails.md` for a more detailed checklist and a "design change" template.

This skill does not currently require `assets/`.
