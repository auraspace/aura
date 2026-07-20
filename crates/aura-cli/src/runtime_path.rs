//! Locate or materialize `aura_rt.c` for the C backend link step.
//!
//! Search order:
//! 1. `AURA_RUNTIME` env (file path)
//! 2. Monorepo / cwd candidates (dev workflow)
//! 3. Next to the `aura` binary (`share/aura/aura_rt.c`, `aura_rt.c`)
//! 4. User cache written from the embedded copy shipped in the CLI

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
    let mut out = Vec::new();
    // In-tree when developing from the monorepo.
    out.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/aura_rt.c"));
    out.push(PathBuf::from("runtime/aura_rt.c"));
    out.push(PathBuf::from("../runtime/aura_rt.c"));
    out.push(PathBuf::from("../../runtime/aura_rt.c"));

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

/// Write embedded runtime to the user cache if missing or stale.
fn materialize_embedded() -> Result<PathBuf, String> {
    let path = cache_file();
    if path.is_file() {
        if let Ok(existing) = fs::read_to_string(&path) {
            if existing == EMBEDDED_RUNTIME_C {
                return Ok(path);
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("error: create runtime cache {}: {e}", parent.display()))?;
    }
    fs::write(&path, EMBEDDED_RUNTIME_C)
        .map_err(|e| format!("error: write runtime cache {}: {e}", path.display()))?;
    Ok(path)
}

/// For tests: ensure resolve always succeeds (embedded fallback).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_nonempty() {
        assert!(EMBEDDED_RUNTIME_C.contains("aura_println"));
        assert!(EMBEDDED_RUNTIME_C.contains("aura_read_file"));
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
        assert!(Path::new(&a).is_file());
    }
}
