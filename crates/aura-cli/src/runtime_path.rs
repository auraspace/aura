//! Locate or materialize `runtime.c` for the C backend link step.
//!
//! Search order:
//! 1. `AURA_RUNTIME` env (file path)
//! 2. Monorepo / cwd candidates (dev workflow)
//! 3. Next to the `aura` binary (`share/aura/runtime/runtime.c`, `runtime.c`)
//! 4. User cache written from the embedded copy shipped in the CLI

use std::env;
use std::fs;
use std::path::PathBuf;

/// Exact runtime sources linked into every user binary (compile-time embed).
pub const EMBEDDED_RUNTIME_C: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/runtime.c"
));

const EMBEDDED_RUNTIME_FILES: &[(&str, &str)] = &[
    (
        "aura_ffi.h",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/aura_ffi.h"
        )),
    ),
    ("runtime.c", EMBEDDED_RUNTIME_C),
    (
        "src/core/preamble.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/core/preamble.c"
        )),
    ),
    (
        "src/core/core.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/core/core.c"
        )),
    ),
    (
        "src/core/string.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/core/string.c"
        )),
    ),
    (
        "src/encoding/encoding.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/encoding/encoding.c"
        )),
    ),
    (
        "src/encoding/json.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/encoding/json.c"
        )),
    ),
    (
        "src/crypto/crypto.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/crypto/crypto.c"
        )),
    ),
    (
        "src/io/dns.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/dns.c"
        )),
    ),
    (
        "src/io/io_file.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/io_file.c"
        )),
    ),
    (
        "src/io/io_tcp.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/io_tcp.c"
        )),
    ),
    (
        "src/io/io_udp.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/io_udp.c"
        )),
    ),
    (
        "src/io/io_websocket.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/io_websocket.c"
        )),
    ),
    (
        "src/io/io_tls.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/io_tls.c"
        )),
    ),
    (
        "src/http/http_parser.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/http/http_parser.c"
        )),
    ),
    (
        "src/http/url.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/http/url.c"
        )),
    ),
    (
        "src/http/mime.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/http/mime.c"
        )),
    ),
    (
        "src/http/http_response.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/http/http_response.c"
        )),
    ),
    (
        "src/http/http_connection.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/http/http_connection.c"
        )),
    ),
    (
        "src/stdlib/stdlib_io_fs.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/stdlib/stdlib_io_fs.c"
        )),
    ),
    (
        "src/stdlib/fs.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/stdlib/fs.c"
        )),
    ),
    (
        "src/core/exceptions.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/core/exceptions.c"
        )),
    ),
    (
        "src/memory/gc.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/memory/gc.c"
        )),
    ),
    (
        "src/memory/ownership.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/memory/ownership.c"
        )),
    ),
    (
        "src/ffi/ffi.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/ffi/ffi.c"
        )),
    ),
    (
        "src/ffi/abi_race.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/ffi/abi_race.c"
        )),
    ),
    (
        "src/task/task_frame.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/task/task_frame.c"
        )),
    ),
    (
        "src/task/task_executor.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/task/task_executor.c"
        )),
    ),
    (
        "src/io/io_operations.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/io/io_operations.c"
        )),
    ),
    (
        "src/task/task_channel.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/task/task_channel.c"
        )),
    ),
    (
        "src/core/process.c",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/src/core/process.c"
        )),
    ),
];

const AURA_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve a filesystem path to `runtime.c` for `cc`.
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

/// Resolve a prebuilt runtime archive when requested, otherwise use the
/// embedded source compatibility path.
pub fn resolve_runtime_input() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("AURA_RUNTIME_LIB") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path.canonicalize().unwrap_or(path));
        }
        return Err(format!(
            "error: AURA_RUNTIME_LIB is set but is not a file: {}",
            path.display()
        ));
    }
    resolve_runtime_c()
}

fn disk_candidates() -> Vec<PathBuf> {
    let mut out = vec![
        // In-tree when developing from the monorepo.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/runtime.c"),
        PathBuf::from("runtime/runtime.c"),
        PathBuf::from("../runtime/runtime.c"),
        PathBuf::from("../../runtime/runtime.c"),
    ];

    // Alongside installed binary (optional layout from package-release).
    if let Ok(exe) = env::current_exe() {
        // Keep both paths: launchers may expose a symlink, while release
        // assets live beside the resolved executable.
        let mut executables = vec![exe.clone()];
        if let Ok(resolved) = exe.canonicalize() {
            if resolved != exe {
                executables.push(resolved);
            }
        }
        for executable in executables {
            if let Some(dir) = executable.parent() {
                out.push(dir.join("runtime.c"));
                out.push(dir.join("runtime/runtime.c"));
                out.push(dir.join("../share/aura/runtime/runtime.c"));
                out.push(dir.join("share/aura/runtime/runtime.c"));
            }
        }
    }
    out
}

fn cache_file() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("aura")
            .join(AURA_VERSION)
            .join("runtime.c");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("aura")
            .join(AURA_VERSION)
            .join("runtime.c");
    }
    env::temp_dir().join(format!("aura-{AURA_VERSION}-runtime.c"))
}

fn fallback_cache_file() -> PathBuf {
    env::temp_dir()
        .join(format!("aura-{AURA_VERSION}-{}", std::process::id()))
        .join("runtime.c")
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
    let root = path
        .parent()
        .ok_or_else(|| format!("error: runtime path has no parent: {}", path.display()))?;
    let complete = EMBEDDED_RUNTIME_FILES.iter().all(|(relative, content)| {
        fs::read_to_string(root.join(relative))
            .map(|existing| existing == *content)
            .unwrap_or(false)
    });
    if complete {
        return Ok(path.to_path_buf());
    }

    fs::create_dir_all(root)
        .map_err(|e| format!("error: create runtime cache {}: {e}", root.display()))?;
    for (relative, content) in EMBEDDED_RUNTIME_FILES {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "error: create embedded runtime directory {}: {e}",
                    parent.display()
                )
            })?;
        }
        fs::write(&destination, content).map_err(|e| {
            format!(
                "error: write embedded runtime {}: {e}",
                destination.display()
            )
        })?;
    }
    Ok(path.to_path_buf())
}

/// For tests: ensure resolve always succeeds (embedded fallback).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_nonempty() {
        let embedded = EMBEDDED_RUNTIME_FILES
            .iter()
            .map(|(_, source)| *source)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(embedded.contains("aura_println"));
        assert!(embedded.contains("aura_read_file"));
        assert!(embedded.contains("aura_try_read_file"));
        assert!(embedded.contains("aura_try_write_file"));
        assert!(embedded.contains("aura_read_line"));
        assert!(embedded.contains("aura_read_all_stdin"));
        assert!(embedded.contains("aura_exit"));
        assert!(embedded.contains("src/io/io_tls.c"));
    }

    #[test]
    fn resolve_ok() {
        let p = resolve_runtime_c().expect("runtime path");
        assert!(p.is_file(), "{}", p.display());
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("src/memory/gc.c"));
        assert!(p.parent().unwrap().join("src/memory/gc.c").is_file());
        assert!(p.parent().unwrap().join("src/io/io_tls.c").is_file());
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
        let primary = blocked_parent.join("runtime.c");
        let fallback = root.join("fallback").join("runtime.c");

        let path = materialize_embedded_from(&primary, &fallback).expect("fallback runtime path");

        assert_eq!(path, fallback);
        assert_eq!(fs::read_to_string(&path).unwrap(), EMBEDDED_RUNTIME_C);
        assert!(path.parent().unwrap().join("src/core/preamble.c").is_file());
        let _ = fs::remove_dir_all(root);
    }
}
