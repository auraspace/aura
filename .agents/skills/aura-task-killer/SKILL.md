---
name: aura-task-killer
description: Execute Aura repository plan tasks end-to-end. Use when asked to work through the TODO checklists in docs/plan (pick next task, implement it, run appropriate verification, then mark the TODO as done).
---

# Aura Task Killer

## Workflow (read plan → do task → mark done)

### 1) Locate the plan

- Default plan root: `docs/plan/`
- Index: `docs/PLAN.md`

If the user specifies a phase, open that phase file directly (e.g. `docs/plan/04-phase3-typeck.md`).

### 2) Select the next TODO item

Selection rules:

- If the user names a phase/topic, pick the first matching unchecked item in that file.
- Otherwise, scan plan files in lexicographic order and pick the first unchecked checkbox line.
- Treat each checkbox line as the unit of work; do not check it off until acceptance is met.

Helper (read-only):

```bash
python3 .agents/skills/aura-task-killer/scripts/next_todo.py
```

### 3) Execute the task

- Keep changes minimal and aligned with the contract docs:
  - `docs/ARCHITECTURE.md`
  - `docs/FOLDER_STRUCTURE.md`
  - `docs/SYNTAX_DESIGN.md`
- If the task changes syntax/semantics, compiler pipeline stages, runtime ABI/behavior, or repo layout, update the corresponding doc in the same change.
- Prefer the smallest verification that increases confidence (build/test/format), and avoid “fixing unrelated issues”.

If the task is about language/runtime/compiler design, also apply: `$aura-dev-guardrails`.

### 4) Mark the TODO done (only after acceptance)

- Flip the checkbox from `- [ ]` to `- [x]`.
- Append ` (done YYYY-MM-DD)` using today’s date in local timezone.
- If useful, append a short note (1 clause) about what shipped (keep it on the same line).

Conventions: read `.agents/skills/aura-task-killer/references/plan-files.md` if you need consistency.

### 5) If scope changes, update the plan

- If you discover missing work needed to complete the phase, add new TODO items under the most relevant phase file in `docs/plan/`.
- Keep TODOs small and verifiable (one checkbox = one acceptance boundary).

