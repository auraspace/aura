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

### Minimal registry MVP (future) — **GitHub-backed** (RFC-005 §6.6, 2026-07-21)

1. **Index:** GitHub repo `auraspace/crates-index` (sparse `packages/…/versions.json`) — not a custom SaaS.
2. **Semver:** caret/default ranges in `aura.toml`; resolver → pin exact versions in `aura.lock`.
3. **Fetch:** download `.crate` from package **GitHub Release** assets; verify sha256.
4. **Lock format:** `name = { version = "…", checksum = "…", source = "registry" }` (alias for default `registry+github:…`); path deps unchanged.
5. **Direct GitHub:** `{ github = "owner/repo", tag = "v1.0.0" }` → lock pins `rev` + checksum (K1b).

### Explicit non-goals now

- Live GitHub index / publish automation (design only until K1/K2)
- Workspaces as a first-class feature (path graphs already cover nested monorepos)

## Next after C8b

- Optional: version field on path deps for documentation only
- ~~Registry lock schema~~ → **C8k done**
- K1: GitHub index client + tarball cache + semver (see RFC-005 §11)
