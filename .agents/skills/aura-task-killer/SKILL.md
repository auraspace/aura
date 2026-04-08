---
name: aura-task-killer
description: Execute Aura repository plan tasks end-to-end. Supports single-task, multi-task, and loop modes with automated commits.
---

# Aura Task Killer

## Workflow (Selection → Cycle → Commit → Loop/Exit)

This skill is designed to automate the progression through the Aura project plan. It operates in a cycle that can be repeated based on the selected execution mode.

### 0) Selection Mode (Pick how many tasks to execute)

Before starting work, ask the user for their preferred execution mode:
- **1 task**: Execute exactly one next task and stop.
- **n tasks**: Execute exactly `n` consecutive tasks and stop.
- **Loop**: Continue executing tasks until the current phase/objective is completed or the user asks to stop.

### 1) Preparation: Locate the plan

- Default plan root: `docs/plan/`
- Index: `docs/PLAN.md`
- If a phase is specified, open that file directly (e.g., `docs/plan/04-phase3-typeck.md`).

---

## The Work Cycle

Repeat the following steps (2-5) for each task according to the selected mode.

### 2) Select the next TODO item

Selection rules:
- Pick the first matching unchecked item (`- [ ]`) in the target phase file.
- Treat each checkbox line as a single unit of work.

Helper (read-only):
```bash
python3 .agents/skills/aura-task-killer/scripts/next_todo.py
```

### 3) Execute & Verify

- **Implement**: Keep changes minimal and aligned with contract docs (`docs/ARCHITECTURE.md`, `docs/SYNTAX_DESIGN.md`).
- **Verify**: Run the smallest verification that increases confidence (build, test, or lint).
- **Guardrails**: Apply `$aura-dev-guardrails` if the task touches language design or runtime.

### 4) Finalize Task (Mark Done)

- Flip the checkbox from `- [ ]` to `- [x]`.
- Append completion info: ` (done YYYY-MM-DD)`.
- Keep the task text stable for searchability.

### 5) Commit Changes

- Create a single commit containing the implementation and the updated plan file.
- **Format**: `<scope>: <description>` (e.g., `typeck: add string literal validation`).
- Include related tests and design doc updates in this same commit.

---

### 6) Flow Control (Next Action)

Evaluate your progress based on the mode selected in **Step 0**:

- **If Mode = 1 task**: Transition to Step 7 and then STOP.
- **If Mode = n tasks**:
  - Decrement your task counter.
  - If counter > 0, return to **Step 2**.
  - If counter = 0, transition to Step 7 and then STOP.
- **If Mode = Loop**:
  - Check for remaining unchecked tasks in the objective/phase.
  - If tasks remain, return to **Step 2**.
  - If all tasks are done, transition to Step 7 and then STOP.

### 7) Plan Maintenance

- If you discover missing work needed to complete the phase, add new TODO items under the most relevant phase file in `docs/plan/`.
- Keep TODOs small and verifiable.
