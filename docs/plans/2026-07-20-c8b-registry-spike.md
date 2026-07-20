# C8b — Path lock polish + registry spike

| Field      | Value                                    |
| ---------- | ---------------------------------------- |
| **Opened** | 2026-07-20                               |
| **After**  | C7 batch + C8a generic Map               |
| **Goal**   | Harden path `aura.lock`; sketch registry |

## Done this slice

- **Path existence check:** `verify_lock_against_toml` ensures every lock entry path is a directory with `aura.toml` (direct + transitive).
- **Docs:** this note + debts update for registry non-goals.

## Registry (not implemented)

RFC-005 still describes the full package manager. Near-term path-only graph remains enough for monorepo + std.

### Minimal registry MVP (future)

1. **Index:** `GET /api/v1/crates/{name}` → versions + yank flags + checksums.
2. **Semver:** caret/default ranges in `aura.toml`; resolver → pin exact versions in `aura.lock`.
3. **Fetch:** download crate tarball to cache; verify sha256.
4. **Lock format:** extend beyond `name = "path"` to `name = { version = "…", checksum = "…", source = "registry" }` while keeping path deps.

### Explicit non-goals now

- Git dependencies
- Publish / `aura add`
- Workspaces as a first-class feature (path graphs already cover nested monorepos)

## Next after C8b

- Optional: version field on path deps for documentation only
- Registry client + lock schema (new compiler batch when needed)
