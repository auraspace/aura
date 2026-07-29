//! Locate the in-tree / installed `std/<leaf>` packages (io, assert, collections, net).
//!
//! Search order:
//! 1. `AURA_STD` env (directory that **contains** `io/`, `assert/`, …)
//! 2. Walk up from the package root looking for `std/<leaf>/aura.toml`
//! 3. Next to the `aura` binary (`share/aura/std/<leaf>`)
//! 4. `$AURA_HOME/current/share/aura/std/<leaf>` (and default `~/.aura`)
//! 5. Monorepo path baked at compile time (`crates/aura-cli/../../std`)
//! 6. Materialize embedded std sources into the user cache

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const AURA_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve `std/<leaf>` (e.g. leaf `"io"` → package `std.io`).
pub fn find_std_package_dir(from: &Path, leaf: &str) -> Option<PathBuf> {
    for c in disk_candidates(from, leaf) {
        if is_std_pkg_dir(&c) {
            return fs::canonicalize(&c).ok().or(Some(c));
        }
    }

    // Only known alpha packages are embedded; unknown leaves stop at disk search.
    if !matches!(leaf, "io" | "assert" | "collections" | "net") {
        return None;
    }

    materialize_embedded_std()
        .ok()
        .map(|root| root.join(leaf))
        .filter(|p| is_std_pkg_dir(p))
}

fn is_std_pkg_dir(p: &Path) -> bool {
    p.is_dir() && p.join("aura.toml").is_file()
}

fn disk_candidates(from: &Path, leaf: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Ok(std_root) = env::var("AURA_STD") {
        out.push(PathBuf::from(std_root).join(leaf));
    }

    let start = fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf());
    let mut cur = Some(start.as_path());
    while let Some(dir) = cur {
        out.push(dir.join("std").join(leaf));
        cur = dir.parent();
    }

    // Alongside installed binary (release tarball layout).
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(format!("../share/aura/std/{leaf}")));
            out.push(dir.join(format!("share/aura/std/{leaf}")));
            out.push(dir.join(format!("../std/{leaf}")));
            out.push(dir.join(format!("std/{leaf}")));
        }
    }

    // Versioned install home.
    let homes = [
        env::var("AURA_HOME").ok().map(PathBuf::from),
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".aura")),
    ];
    for home in homes.into_iter().flatten() {
        out.push(home.join(format!("current/share/aura/std/{leaf}")));
        out.push(home.join(format!("share/aura/std/{leaf}")));
        out.push(home.join(format!("stdlib/std/{leaf}")));
    }

    // Monorepo when developing / cargo run from workspace.
    out.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../std")
            .join(leaf),
    );
    out.push(PathBuf::from("std").join(leaf));

    out
}

fn cache_std_root() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("aura")
            .join(AURA_VERSION)
            .join("std");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("aura")
            .join(AURA_VERSION)
            .join("std");
    }
    env::temp_dir().join(format!("aura-{AURA_VERSION}-std"))
}

/// Write embedded std packages into the user cache if missing or stale.
pub fn materialize_embedded_std() -> Result<PathBuf, String> {
    let root = cache_std_root();
    for (rel, content) in EMBEDDED_STD_FILES {
        let path = root.join(rel);
        let need_write = match fs::read_to_string(&path) {
            Ok(existing) => existing != *content,
            Err(_) => true,
        };
        if need_write {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("error: create std cache {}: {e}", parent.display()))?;
            }
            fs::write(&path, content)
                .map_err(|e| format!("error: write std cache {}: {e}", path.display()))?;
        }
    }
    Ok(root)
}

/// Embedded alpha std sources (same files as monorepo `std/`).
const EMBEDDED_STD_FILES: &[(&str, &str)] = &[
    (
        "io/aura.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/io/aura.toml"
        )),
    ),
    (
        "io/src/lib.aura",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/io/src/lib.aura"
        )),
    ),
    (
        "assert/aura.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/assert/aura.toml"
        )),
    ),
    (
        "assert/src/lib.aura",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/assert/src/lib.aura"
        )),
    ),
    (
        "collections/aura.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/collections/aura.toml"
        )),
    ),
    (
        "collections/src/lib.aura",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/collections/src/lib.aura"
        )),
    ),
    (
        "net/aura.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/net/aura.toml"
        )),
    ),
    (
        "net/src/lib.aura",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/net/src/lib.aura"
        )),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_has_io() {
        assert!(EMBEDDED_STD_FILES
            .iter()
            .any(|(p, c)| *p == "io/src/lib.aura" && c.contains("package std.io")));
    }

    #[test]
    fn embedded_has_net_primitive_bridge() {
        assert!(EMBEDDED_STD_FILES
            .iter()
            .any(|(p, c)| *p == "net/src/lib.aura" && c.contains("package std.net")));
    }

    #[test]
    fn materialize_and_find_io() {
        let root = materialize_embedded_std().expect("materialize");
        let io = root.join("io");
        assert!(is_std_pkg_dir(&io), "{}", io.display());
        // find from a temp package path should still hit cache fallback
        let tmp = env::temp_dir().join(format!("aura-std-find-{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        let found = find_std_package_dir(&tmp, "io").expect("find io");
        assert!(is_std_pkg_dir(&found), "{}", found.display());
        assert!(found.join("src/lib.aura").is_file());
    }

    #[test]
    fn monorepo_std_if_present() {
        let mono = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std/io");
        if mono.is_dir() {
            let found = find_std_package_dir(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
                "io",
            )
            .expect("monorepo io");
            assert!(found.ends_with("std/io") || found.join("aura.toml").is_file());
        }
    }
}
