# CLI Compatibility Matrix

The following commands are stable in the alpha CLI. User-input/usage errors
return `2`; operational failures return `1` unless a documented domain status
is required (`aura update` uses `3` for revocation); success returns `0`.

| Area            | Commands                                       | JSON contract                                           | Gate                        |
| --------------- | ---------------------------------------------- | ------------------------------------------------------- | --------------------------- |
| Build/run       | `check`, `build`, `run`, `emit-c`, `emit-llvm` | diagnostics use `JsonDiagnostic`                        | Rust + corpus               |
| Tests           | `test`, `bench`, `race`                        | stable test report with case status/duration/diagnostic | `scripts/tests/coverage.sh` |
| Packages        | `add`, `remove`, `update`, `tree`              | `tree --format json` is deterministic                   | registry acceptance         |
| Formatting/docs | `fmt`, `fix`, `doc`, `clean`                   | deterministic output; errors are diagnostics            | CLI unit tests              |
| Toolchain       | `toolchain list                                | current                                                 | switch`                     | `{ok,current,versions}` | `scripts/tests/cli-compatibility.sh` |

The compatibility gate checks all public command names, usage exit classes, and
the machine-readable toolchain/tree shapes on every release rehearsal.
