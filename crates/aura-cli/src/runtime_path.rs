//! Locate or materialize `aura_rt.c` for the C backend link step.
//!
//! Search order:
//! 1. `AURA_RUNTIME` env (file path)
//! 2. Monorepo / cwd candidates (dev workflow)
//! 3. Next to the `aura` binary (`share/aura/aura_rt.c`, `aura_rt.c`)
//! 4. User cache written from the embedded copy shipped in the CLI

use std::env;
use std::fs;
use std::path::PathBuf;

/// Exact runtime sources linked into every user binary (compile-time embed).
pub const EMBEDDED_RUNTIME_C: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/aura_rt.c"
));

const AURA_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve a filesystem path to `aura_rt.c` for `cc`.
pub fn resolve_runtime_c() -> Result<PathBuf, String> {
    if let Ok(p) = env::var("AURA_RUNTIME") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p.canonicalize().unwrap_or(p));
        }
        return Err(format!(
            "error: AURA_RUNTIME is set but not a file: {}",
            p.display()
        ));
    }

    for c in disk_candidates() {
        if c.is_file() {
            return Ok(c.canonicalize().unwrap_or(c));
        }
    }

    materialize_embedded()
}

fn disk_candidates() -> Vec<PathBuf> {
    let mut out = vec![
        // In-tree when developing from the monorepo.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/aura_rt.c"),
        PathBuf::from("runtime/aura_rt.c"),
        PathBuf::from("../runtime/aura_rt.c"),
        PathBuf::from("../../runtime/aura_rt.c"),
    ];

    // Alongside installed binary (optional layout from package-release).
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("aura_rt.c"));
            out.push(dir.join("runtime/aura_rt.c"));
            out.push(dir.join("../share/aura/aura_rt.c"));
            out.push(dir.join("share/aura/aura_rt.c"));
        }
    }
    out
}

fn cache_file() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("aura")
            .join(AURA_VERSION)
            .join("aura_rt.c");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("aura")
            .join(AURA_VERSION)
            .join("aura_rt.c");
    }
    env::temp_dir().join(format!("aura-{AURA_VERSION}-aura_rt.c"))
}

fn fallback_cache_file() -> PathBuf {
    env::temp_dir()
        .join(format!("aura-{AURA_VERSION}-{}", std::process::id()))
        .join("aura_rt.c")
}

/// Write embedded runtime to the user cache if missing or stale.
fn materialize_embedded() -> Result<PathBuf, String> {
    let primary = cache_file();
    let fallback = fallback_cache_file();
    materialize_embedded_from(&primary, &fallback)
}

fn materialize_embedded_from(
    primary: &std::path::Path,
    fallback: &std::path::Path,
) -> Result<PathBuf, String> {
    if primary == fallback {
        return materialize_embedded_at(primary);
    }

    let mut errors = Vec::new();
    for path in [primary, fallback] {
        match materialize_embedded_at(path) {
            Ok(path) => return Ok(path),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "error: unable to materialize embedded runtime; {}",
        errors.join("; ")
    ))
}

fn materialize_embedded_at(path: &std::path::Path) -> Result<PathBuf, String> {
    if path.is_file() {
        if let Ok(existing) = fs::read_to_string(path) {
            if existing == EMBEDDED_RUNTIME_C {
                return Ok(path.to_path_buf());
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("error: create runtime cache {}: {e}", parent.display()))?;
    }
    fs::write(path, EMBEDDED_RUNTIME_C)
        .map_err(|e| format!("error: write runtime cache {}: {e}", path.display()))?;
    Ok(path.to_path_buf())
}

/// For tests: ensure resolve always succeeds (embedded fallback).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_nonempty() {
        assert!(EMBEDDED_RUNTIME_C.contains("aura_println"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_read_file"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_try_read_file"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_try_write_file"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_read_line"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_read_all_stdin"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_exit"));
    }

    #[test]
    fn resolve_ok() {
        let p = resolve_runtime_c().expect("runtime path");
        assert!(p.is_file(), "{}", p.display());
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("aura_gc_alloc"));
    }

    #[test]
    fn materialize_idempotent() {
        let a = materialize_embedded().unwrap();
        let b = materialize_embedded().unwrap();
        assert_eq!(a, b);
        assert!(a.is_file());
    }

    #[test]
    fn materialize_falls_back_when_primary_cache_is_unwritable() {
        let root = env::temp_dir().join(format!(
            "aura-runtime-path-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let blocked_parent = root.join("blocked");
        // File (not dir) so create_dir_all(primary.parent()) fails and we fall back.
        fs::write(&blocked_parent, "not a directory").unwrap();
        let primary = blocked_parent.join("aura_rt.c");
        let fallback = root.join("fallback").join("aura_rt.c");

        let path = materialize_embedded_from(&primary, &fallback).expect("fallback runtime path");

        assert_eq!(path, fallback);
        assert_eq!(fs::read_to_string(path).unwrap(), EMBEDDED_RUNTIME_C);
        let _ = fs::remove_dir_all(root);
    }
}
