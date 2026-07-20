# Aura Project Agents

Custom agents for the Aura compiler and ecosystem.

## Agents

| Agent                     | Focus                                                                                      | Use when                                                     |
| ------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| **Compiler Expert**       | `aura-ast`, `aura-parser`, `aura-lexer`, `aura-sema`, `aura-codegen`; AST, sema, C codegen | Compiler passes, parser/sema bugs, codegen, multi-crate work |
| **Language Spec Expert**  | RFC-000 principles, RFC-001 language, RFC-002 types, RFC-003 memory/concurrency            | New features, type-system design, spec ambiguities, RFCs     |
| **Test & Corpus Manager** | `/corpus`, categories, e2e compile tests, examples                                         | New tests, corpus layout, docs examples, integration tests   |
| **Docs & RFC Specialist** | RFCs 000–013, architecture docs, roadmap                                                   | Writing RFCs/docs, design decisions, planning                |
| **Runtime & Integration** | `aura_rt.c`, RFC-006 runtime, RFC-008 build                                                | Runtime features, C/codegen glue, performance, runtime lib   |
| **Website & Tooling**     | React/TypeScript site, Vite, tooling                                                       | Site updates, dev tools, interactive examples                |

## Quick Select

| Task                     | Agent                             |
| ------------------------ | --------------------------------- |
| Fix parser bug           | Compiler Expert                   |
| Design type feature      | Language Spec Expert              |
| Add test case            | Test & Corpus Manager             |
| Write RFC                | Docs & RFC Specialist             |
| Implement runtime fn     | Runtime & Integration             |
| Update website           | Website & Tooling                 |
| Multi-component refactor | Compiler Expert / parallel agents |

## Usage

```
@compiler-expert Help me implement the type inference pass
@language-spec-expert Design a new generic type constraint syntax
@test-manager Add tests for the new nullable type feature
```

Complex cross-domain work can use multiple agents in parallel or sequence.
Agent count is not fixed — estimate how many are needed from the task scope.

## Split Rules

Split when this file gets hard to scan, domains start overlapping, or detail no longer fits an index.

```
agents/
├── AGENTS.md           # index
├── compiler.md
├── language-design.md
├── testing.md
├── documentation.md
├── runtime.md
└── tooling.md
```

Main file stays an index; domain files hold detail. Link as `See [compiler.md](compiler.md)`.

## Technical Debt

Always record technical debt, temporary workarounds, known incomplete behavior, and deferred follow-ups in [agents/debts.md](agents/debts.md).

Rules:

- Do not leave debt only in chat, commit messages, or scattered `TODO` comments.
- When you introduce or discover debt, append or update an entry in `agents/debts.md` in the same change.
- When you resolve debt, update or remove the matching entry there.
- Prefer short, actionable notes: area, symptom, why deferred, and next step.
