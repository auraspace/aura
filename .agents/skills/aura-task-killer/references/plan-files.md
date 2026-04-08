# Aura plan files conventions

This skill assumes the project plan lives under `docs/plan/` and uses Markdown checkboxes:

- Not started: `- [ ] ...`
- Done: `- [x] ...`

When marking an item complete, prefer to:

- Flip `[ ]` → `[x]`
- Append a completion marker like ` (done YYYY-MM-DD)`
- Keep the item’s text stable so it remains searchable

### Commit Convention

When finishing a task, create a single commit that includes your implementation and the updated plan file. Follow the project's commit message style:

- Format: follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/#summary) using `<type>(<scope>): <description>` when a scope is useful, or `<type>: <description>` when it is not.
- Examples: `fix(typeck): reject invalid string literals`, `docs: update syntax design`
- Type: Use a conventional commit type such as `feat`, `fix`, `docs`, `refactor`, `test`, or `chore`.
- Scope: Usually the crate name (`typeck`, `parser`, `resolver`) or `plan`/`docs`.
- Description: Focus on what was *changed* or *added*.
