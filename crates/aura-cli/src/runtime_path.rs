//! Resolve the target/backend/profile runtime artifact for native builds.
//!
//! Search order is explicit: overrides, installed toolchain, local checkout,
//! dev cache, then source fallback for development/bootstrap only.

use std::env;
use std::fs;
use std::path::PathBuf;

use aura_codegen::Backend;
use sha2::{Digest, Sha256};

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
    (
        "llvm_exceptions.h",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/llvm_exceptions.h"
        )),
    ),
];

const AURA_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeRequest {
    backend: Backend,
    profile: String,
    sanitizer: String,
    lto: String,
    features: String,
    compiler: String,
    rebuild_runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeCandidate {
    path: PathBuf,
    metadata_required: bool,
}

impl RuntimeRequest {
    fn from_env(backend: Backend, detector: Option<bool>) -> Self {
        let profile = runtime_profile();
        Self {
            sanitizer: if backend == Backend::C && profile == "dev" && detector.unwrap_or(true) {
                "address,undefined".into()
            } else {
                "none".into()
            },
            lto: env::var("AURA_RUNTIME_LTO").unwrap_or_else(|_| "off".into()),
            features: env::var("AURA_RUNTIME_FEATURES").unwrap_or_default(),
            compiler: env::var("AURA_RUNTIME_CC")
                .or_else(|_| env::var("CC"))
                .unwrap_or_else(|_| "cc".into()),
            rebuild_runtime: env::var("AURA_REBUILD_RUNTIME")
                .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
                .unwrap_or(false),
            backend,
            profile,
        }
    }
}

/// Resolve the preferred runtime input for a backend.
pub fn resolve_runtime_input(backend: Backend) -> Result<PathBuf, String> {
    resolve_runtime_input_with_rebuild(backend, false)
}

/// Resolve a runtime while allowing an explicit source rebuild for bootstrap.
pub fn resolve_runtime_input_with_rebuild(
    backend: Backend,
    rebuild_runtime: bool,
) -> Result<PathBuf, String> {
    resolve_runtime_input_with_config(backend, rebuild_runtime, None)
}

/// Resolve a runtime with the application's sanitizer setting in its identity.
pub fn resolve_runtime_input_with_config(
    backend: Backend,
    rebuild_runtime: bool,
    detector: Option<bool>,
) -> Result<PathBuf, String> {
    let mut request = RuntimeRequest::from_env(backend, detector);
    request.rebuild_runtime |= rebuild_runtime;
    resolve_runtime_input_with_request(request)
}

fn resolve_runtime_input_with_request(request: RuntimeRequest) -> Result<PathBuf, String> {
    let profile = &request.profile;
    let override_name = match request.backend {
        Backend::C => "AURA_RUNTIME_LIB",
        Backend::Llvm => "AURA_LLVM_RUNTIME_LIB",
        Backend::Cranelift => "AURA_RUNTIME_LIB",
    };
    if let Ok(path) = env::var(override_name) {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(format!(
                "error: {override_name} is set but is not a file: {}",
                path.display()
            ));
        }
        if !is_runtime_archive(&path) {
            return Err(format!(
                "error: {override_name} must point to a runtime archive (.a or .lib): {}",
                path.display()
            ));
        }
        validate_runtime_archive(&path, &request, false)?;
        return Ok(path.canonicalize().unwrap_or(path));
    }

    if !request.rebuild_runtime {
        for candidate in archive_candidates(&request) {
            if candidate.path.is_file()
                && validate_runtime_archive(&candidate.path, &request, candidate.metadata_required)
                    .is_ok()
            {
                return Ok(candidate.path.canonicalize().unwrap_or(candidate.path));
            }
        }
    }

    if source_fallback_allowed(&request) {
        return resolve_runtime_source();
    }
    Err(format!(
        concat!(
            "error: no compatible {} runtime archive for profile `{}`; release builds do not ",
            "compile runtime.c implicitly (use a shipped/local archive or set ",
            "AURA_REBUILD_RUNTIME=1 for an explicit rebuild)"
        ),
        backend_name(request.backend),
        profile
    ))
}

fn source_fallback_allowed(request: &RuntimeRequest) -> bool {
    request.rebuild_runtime || matches!(request.profile.as_str(), "dev" | "debug" | "test")
}

fn runtime_profile() -> String {
    env::var("AURA_RUNTIME_PROFILE").unwrap_or_else(|_| "dev".into())
}

fn archive_candidates(request: &RuntimeRequest) -> Vec<RuntimeCandidate> {
    let archive = match request.backend {
        Backend::C => "libaurart.a",
        Backend::Llvm => "libaurart-llvm.a",
        Backend::Cranelift => "libaurart.a",
    };
    let target = current_target();
    let mut installed_roots = Vec::new();
    let mut local_roots = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime"),
        PathBuf::from("runtime"),
    ];
    let mut out = Vec::new();
    if let Ok(exe) = env::current_exe() {
        let mut executables = vec![exe.clone()];
        if let Ok(resolved) = exe.canonicalize() {
            if resolved != exe {
                executables.push(resolved);
            }
        }
        for executable in executables {
            if let Some(dir) = executable.parent() {
                installed_roots.push(dir.join("../share/aura/runtime"));
                installed_roots.push(dir.join("share/aura/runtime"));
                local_roots.push(dir.join("runtime"));
                local_roots.push(dir.to_path_buf());
            }
        }
    }
    for root in installed_roots {
        push_archive_candidates(&mut out, &root, &target, request, archive, true);
        out.push(RuntimeCandidate {
            path: root.join(archive),
            metadata_required: true,
        });
    }
    for root in cache_roots() {
        out.push(RuntimeCandidate {
            path: root
                .join(&target)
                .join(backend_name(request.backend))
                .join(&request.profile)
                .join(runtime_cache_key(request))
                .join(archive),
            metadata_required: true,
        });
    }
    for root in local_roots {
        push_archive_candidates(&mut out, &root, &target, request, archive, false);
        // Legacy local/toolchain archive layout remains supported.
        out.push(RuntimeCandidate {
            path: root.join(archive),
            metadata_required: false,
        });
    }
    out
}

fn push_archive_candidates(
    out: &mut Vec<RuntimeCandidate>,
    root: &std::path::Path,
    target: &str,
    request: &RuntimeRequest,
    archive: &str,
    metadata_required: bool,
) {
    let profile_root = root
        .join(target)
        .join(backend_name(request.backend))
        .join(&request.profile);
    // Keep the original profile path as the default release layout, then use
    // a sanitizer-specific sibling for profiles that have multiple variants.
    for path in [
        profile_root.join(archive),
        profile_root
            .join(runtime_sanitizer_dir(&request.sanitizer))
            .join(archive),
    ] {
        out.push(RuntimeCandidate {
            path,
            metadata_required,
        });
    }
}

fn runtime_sanitizer_dir(sanitizer: &str) -> String {
    if sanitizer == "address,undefined" {
        "asan-ubsan".into()
    } else if sanitizer.is_empty() || sanitizer == "none" {
        "none".into()
    } else {
        sanitizer.replace(',', "-")
    }
}

fn cache_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let base = if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".cache")
    } else {
        env::temp_dir()
    };
    roots.push(base.join("aura").join(AURA_VERSION).join("runtime"));
    roots
}

fn runtime_cache_key(request: &RuntimeRequest) -> String {
    let feature_key = if request.features.is_empty() {
        "none".to_owned()
    } else {
        Sha256::digest(request.features.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    format!(
        "abi-{}-san-{}-lto-{}-cc-{}-features-{}",
        aura_codegen::RUNTIME_ABI_VERSION,
        request.sanitizer.replace(',', "-"),
        request.lto,
        Sha256::digest(request.compiler.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        feature_key
    )
}

fn backend_name(backend: Backend) -> &'static str {
    match backend {
        Backend::C => "c",
        Backend::Llvm => "llvm",
        Backend::Cranelift => "c",
    }
}

fn current_target() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os}-{arch}")
}

fn current_target_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

fn is_runtime_archive(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("a" | "lib")
    )
}

fn validate_runtime_archive(
    path: &std::path::Path,
    request: &RuntimeRequest,
    metadata_required: bool,
) -> Result<(), String> {
    let metadata_path = PathBuf::from(format!("{}.meta", path.display()));
    if !metadata_path.is_file() {
        if metadata_required {
            return Err(format!(
                "error: runtime metadata is required for shared/cache archive: {}",
                metadata_path.display()
            ));
        }
        return Ok(()); // Legacy local archives predate runtime metadata.
    }
    let text = fs::read_to_string(&metadata_path).map_err(|error| {
        format!(
            "error: read runtime metadata {}: {error}",
            metadata_path.display()
        )
    })?;
    let mut values = std::collections::HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "error: malformed runtime metadata: {}",
                metadata_path.display()
            ));
        };
        values.insert(key, value);
    }
    let expected_backend = backend_name(request.backend);
    let expected_target = current_target();
    let expected_triple = current_target_triple();
    for (key, expected) in [
        ("schema", "1".to_owned()),
        ("target", expected_target),
        ("backend", expected_backend.to_owned()),
        ("profile", request.profile.to_owned()),
        ("sanitizer", request.sanitizer.to_owned()),
        ("lto", request.lto.to_owned()),
        (
            "features",
            if request.features.is_empty() {
                "none".to_owned()
            } else {
                request.features.to_owned()
            },
        ),
        (
            "runtime_abi_version",
            aura_codegen::RUNTIME_ABI_VERSION.to_string(),
        ),
        (
            "runtime_abi_identity",
            aura_codegen::RUNTIME_ABI_ID.to_owned(),
        ),
    ] {
        if values.get(key).copied() != Some(expected.as_str()) {
            return Err(format!(
                "error: runtime metadata mismatch for `{key}` in {}",
                metadata_path.display()
            ));
        }
    }
    if let Some(expected_triple) = expected_triple {
        if values.get("target_triple").copied() != Some(expected_triple) {
            return Err(format!(
                "error: runtime target triple mismatch in {}",
                metadata_path.display()
            ));
        }
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("error: read runtime archive {}: {error}", path.display()))?;
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if values.get("sha256").copied() != Some(digest.as_str()) {
        return Err(format!(
            "error: runtime archive checksum mismatch: {}",
            path.display()
        ));
    }
    Ok(())
}

fn resolve_runtime_source() -> Result<PathBuf, String> {
    if let Ok(p) = env::var("AURA_RUNTIME") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p.canonicalize().unwrap_or(p));
        }
        return Err(format!(
            "error: AURA_RUNTIME is set but is not a file: {}",
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

    fn test_request() -> RuntimeRequest {
        RuntimeRequest {
            backend: Backend::C,
            profile: "dev".into(),
            sanitizer: "none".into(),
            lto: "off".into(),
            features: String::new(),
            compiler: "cc".into(),
            rebuild_runtime: false,
        }
    }

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
    fn legacy_local_archive_without_metadata_remains_accepted() {
        let path = env::temp_dir().join(format!(
            "aura-legacy-runtime-{}-{}.a",
            std::process::id(),
            unique_test_suffix()
        ));

        validate_runtime_archive(&path, &test_request(), false)
            .expect("legacy local archive should not require metadata");
    }

    #[test]
    fn shared_or_cached_archive_without_metadata_is_rejected() {
        let path = env::temp_dir().join(format!(
            "aura-shared-runtime-{}-{}.a",
            std::process::id(),
            unique_test_suffix()
        ));

        let error = validate_runtime_archive(&path, &test_request(), true)
            .expect_err("shared/cache archive should require metadata");
        assert!(error.contains("metadata is required"));
    }

    fn unique_test_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    }

    #[test]
    fn resolve_ok() {
        let p = resolve_runtime_source().expect("runtime path");
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

    #[test]
    fn source_fallback_is_dev_only_unless_explicitly_rebuilt() {
        let release = RuntimeRequest {
            backend: Backend::C,
            profile: "release".into(),
            sanitizer: "none".into(),
            lto: "off".into(),
            features: String::new(),
            compiler: "cc".into(),
            rebuild_runtime: false,
        };
        assert!(!source_fallback_allowed(&release));

        let mut explicit = release.clone();
        explicit.rebuild_runtime = true;
        assert!(source_fallback_allowed(&explicit));

        let mut dev = release;
        dev.profile = "dev".into();
        assert!(source_fallback_allowed(&dev));
    }
}
