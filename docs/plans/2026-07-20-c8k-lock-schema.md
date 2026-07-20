# C8k — Registry lock schema v0

| Field      | Value                                       |
| ---------- | ------------------------------------------- |
| **Opened** | 2026-07-20                                  |
| **After**  | C8b path lock polish                        |
| **Goal**   | Parse extended `aura.lock` forms (no fetch) |

## Done

- **LockEntry:** `path` / `version` / `source` / `checksum` fields.
- **Parse:** legacy `name = "path"` and inline table  
  `name = { path = "…", version = "…", source = "path"|"registry", checksum = "…" }`.
- **Verify:** path entries still require on-disk `aura.toml`; registry entries skip path check (schema-only until client exists).
- **Write:** still emits path string form for resolved path graphs.

## Not done (follow-ups)

- Semver resolver / caret ranges in `aura.toml`
- Registry HTTP client + tarball cache
- Writing registry pins into lock from resolve
