# Phase 5 — Backend and Target Policy (v1.1)

_Last updated: 2026-04-09_

## Goal

Keep target handling and backend selection centralized so the compiler can grow to more targets without scattering policy logic.

## Priority

This is later in v1.1 because it benefits from the earlier frontend/runtime tightening, and it should preserve the target policy that v1.0 already established.

## Scope

- Refine target capability reporting.
- Keep placeholder targets failing fast with clear diagnostics.
- Make backend selection and capability checks easier to follow.
- Avoid ad hoc triple checks in CLI and backend code.

## Implementation Notes

- Treat target support as a structured policy model rather than a string comparison.
- Ensure the CLI asks the target layer for capabilities instead of re-deriving them.
- Keep unsupported-target behavior consistent between compile, emit, and link paths.
- Add tests for both supported and placeholder-only targets so policy drift is visible.
- Make the linker choice a target responsibility, not a CLI switch statement.

## Policy Themes

- Targets should be described by one consistent metadata model.
- Capability checks should happen before backend construction or code generation.
- Linking policy should remain in the linker layer, not the CLI.

## TODO

- [ ] Define the exact target descriptor fields needed by CLI, codegen, and linker.
- [ ] Document supported capability queries such as codegen, object emission, and linking.
- [ ] Add rejection-path tests for unsupported emit modes and placeholder-only targets.
- [ ] Document any target policy states beyond supported, placeholder-only, and unknown.
- [ ] Verify linker selection comes from target metadata rather than raw string branches.
- [ ] Add a test that proves the CLI fails before backend construction for unsupported targets.

## Acceptance

- [ ] Supported targets are resolved through one stable API.
- [ ] Unsupported targets fail early with clear target-specific diagnostics.
- [ ] Backend and linker policy stay decoupled from CLI string handling.
- [ ] Capability checks are visible in tests and easy to extend for new targets.
