# C8b — Path lock polish + registry spike

| Field      | Value                                    |
| ---------- | ---------------------------------------- |
| **Opened** | 2026-07-20                               |
| **After**  | C7 batch + C8a generic Map               |
| **Goal**   | Harden path `aura.lock`; sketch registry |

## Done this slice

- **Path existence check:** `verify_lock_against_toml` ensures every lock entry path is a directory with `aura.toml` (direct + transitive).
- **Docs:** this note + debts update for registry non-goals.

## Registry status

The original path-only spike is complete. Current code consumes locked origin
metadata and archives over HTTPS with semver pinning, checksum verification,
cache extraction, and offline fixtures. Live origin publication/authentication,
direct GitHub sources, and workspaces remain deferred; a proxy is explicitly
later work after the read contract stabilizes. See RFC-005 and the v0.1.1-alpha
contract matrix.

### Registry MVP — **GitHub-backed** (RFC-005 §6.6, 2026-07-21)

1. **Origin:** public Git repository with `aura.toml` and immutable semver tags — no custom SaaS or index.
2. **Semver:** caret/default ranges in `aura.toml`; resolver → pin exact versions in `aura.lock`.
3. **Read:** resolve `vX.Y.Z` tags and fetch the tagged source/archive directly; verify sha256. Locked consumption is implemented; live publication remains open.
4. **Lock format:** `name = { version = "…", checksum = "…", source = "git+https://…", rev = "…" }`; path deps unchanged.
5. **Direct GitHub:** `{ github = "owner/repo", tag = "v1.0.0" }` → lock pins `rev` + checksum (K1b).

### Explicit non-goals now

- Live Git-tag publication helper/automation (design only until K2)
- Proxy or mirror service (wait for the origin read contract)
- Workspaces as a first-class feature (path graphs already cover nested monorepos)

## Historical next steps

- Optional: version field on path deps for documentation only
- ~~Registry lock schema~~ → **C8k done**
- K1: GitHub index client + tarball cache + semver — landed as the bounded read/verify/cache path; live publication remains open
