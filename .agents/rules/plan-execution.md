---
trigger: always_on
glob: "**/*"
description: "Rules for following the implementation plans and roadmaps"
---

# Plan Execution Rules

To ensure structured and prioritized development of the Aura language, all tasks must be executed in accordance with the strategic roadmaps defined in the `plans/` directory.

## 📋 General Principles

1.  **Context Recognition**: Before initiating any implementation, identify which roadmap from `plans/` is currently active (e.g., Plan 1: Vertical Slice).
2.  **Phase Adherence**: Each roadmap is divided into phases. All tasks and PRs must align with the objectives of the current phase. Avoid implementing features reserved for future phases.
3.  **Scope Control**: Prioritize the specific focus of the active plan (e.g., Speed for Vertical Slice, Correctness for Type-Safe Core). Do not over-engineer beyond the current phase's needs.
4.  **Traceability**: When starting a new task, explicitly state which Plan and Phase it belongs to.

## 🛠 Execution Workflow

1.  **Check Todos**: Inspect the `todos/` directory for any active or pending task files.
2.  **Task Acquisition**:
    - If a task exists, pick the next uncompleted item.
    - If no task file exists, create a new one in `todos/` (e.g., `todo_phase1.md`) based on the current Phase of the active roadmap in `plans/`.
3.  **Plan Selection**: If no plan is explicitly active, default to **Plan 1: Vertical Slice** for initial prototyping.
4.  **Step Breakdown**: Break down the USER_REQUEST or the selected todo into sub-tasks.
5.  **Execution**: Implement the task following the architectural constraints.
6.  **Progress Tracking**: Mark tasks as completed in the corresponding `todos/` file immediately after implementation and verification.
7.  **Validation**: After completion, verify that the work maintains the architectural integrity defined in the roadmap.

## ⚠️ Constraints

- Never skip phases unless instructed.
- Ensure `docs/` and `src/` remain consistent with the current implementation phase.
