# Aura Dev Guardrails (Detailed Checklist)

This file is intentionally more verbose than `SKILL.md`. Load this reference when you need a concrete checklist or when a change is likely to affect the language design contract.

## Core Contract Docs (Source of Truth)

- `docs/ARCHITECTURE.md` (compiler pipeline + runtime embedding + exception unwinding approach)
- `docs/FOLDER_STRUCTURE.md` (repo layout + multi-target scaling)
- `docs/SYNTAX_DESIGN.md` (keywords, grammar sketch, OOP model, exceptions)

If implementation changes reality, update the docs in the same change.

## "No Silent Divergence" Checklist

Run this checklist before finalizing a change:

- Does the change introduce a new keyword, new syntax form, or change semantics of an existing form?
- Does it change the compiler stage boundaries (frontend/middle-end/backend) or add/remove an IR?
- Does it alter runtime responsibilities, ABI surface, object layout, or exception behavior?
- Does it add a new crate, move a crate, or add a top-level directory?
- Does it introduce a new target/backend or change how linking works on macOS?

If YES to any item: the corresponding doc must be updated.

## Design Change Template (Use in PR/Notes)

Fill in (briefly):

- **Change summary**:
- **Motivation**:
- **Impacted contract docs**: (ARCHITECTURE / FOLDER_STRUCTURE / SYNTAX)
- **Compatibility**:
  - Source compatibility:
  - Binary/runtime ABI compatibility:
- **Invariants preserved**:
- **New invariants** (if any):
- **Tests/fixtures updated**:

## Exception-Specific Checklist

When touching `throw`, `try/catch/finally`, or runtime unwinding:

- Ensure `finally` executes on all exits (normal/return/throw).
- Ensure ARC/cleanup actions are modeled as explicit cleanups (not relying on OS EH in MVP).
- Keep interop rules clear (do not allow exceptions to cross C boundaries without a deliberate design update).

