use crate::package::load_package;
use crate::package::toml::parse_aura_toml;
use std::fs;
use std::io::Write;
use std::path::Path;

fn write_tree(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
}

#[test]
fn parse_manifest_keys() {
    let t = parse_aura_toml(
        r#"
[package]
name = "demo.multi"

[[bin]]
name = "multi"
path = "src"
"#,
    )
    .unwrap();
    assert_eq!(t.package_name.as_deref(), Some("demo.multi"));
    assert_eq!(t.bin_name.as_deref(), Some("multi"));
    assert_eq!(t.bin_path.as_deref(), Some("src"));
}

#[test]
fn merge_two_files() {
    let root = std::env::temp_dir().join(format!("aura-pkg-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    write_tree(
        &root,
        &[
            (
                "aura.toml",
                r#"[package]
name = "demo.multi"

[[bin]]
name = "multi"
path = "src"
"#,
            ),
            (
                "src/util.aura",
                r#"package demo.multi

fun square(x: Int): Int {
  return x * x
}
"#,
            ),
            (
                "src/main.aura",
                r#"package demo.multi

fun main() {
  println(square(4))
}
"#,
            ),
        ],
    );
    let pkg = load_package(&root.join("aura.toml")).expect("load");
    assert_eq!(pkg.package, "demo.multi");
    assert_eq!(pkg.bin_name, "multi");
    // App sources only: main + util (std.io may be merged as extra sources via auto-prelude).
    assert!(
        pkg.sources.len() >= 2,
        "expected at least app sources, got {}",
        pkg.sources.len()
    );
    let names: Vec<_> = pkg
        .ast
        .functions
        .iter()
        .map(|f| f.name.name.as_str())
        .collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"square"));
    // Auto-prelude merges std.io free functions when the std tree is discoverable.
    assert!(
        names.contains(&"println"),
        "expected std.io auto-prelude println, got {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reject_package_mismatch() {
    let root = std::env::temp_dir().join(format!("aura-pkg-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    write_tree(
        &root,
        &[
            ("a.aura", "package foo\nfun a() {}\n"),
            ("b.aura", "package bar\nfun b() {}\n"),
        ],
    );
    let err = load_package(&root).unwrap_err();
    assert!(err.contains("package mismatch"), "{err}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn locate_span_in_second_file() {
    let root = std::env::temp_dir().join(format!("aura-pkg-loc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    write_tree(
        &root,
        &[
            ("a.aura", "package p\nfun a(): Int { return 1 }\n"),
            ("b.aura", "package p\nfun b(): Int { return 2 }\n"),
        ],
    );
    let pkg = load_package(&root).unwrap();
    let b_fn = pkg
        .ast
        .functions
        .iter()
        .find(|f| f.name.name == "b")
        .unwrap();
    let (path, _src, local) = pkg.locate(b_fn.name.span);
    assert!(path.ends_with("b.aura"), "{path}");
    assert!(local.start < 20);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reject_duplicate_fun() {
    let root = std::env::temp_dir().join(format!("aura-pkg-dup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    write_tree(
        &root,
        &[
            ("a.aura", "package p\nfun shared(): Int { return 1 }\n"),
            ("b.aura", "package p\nfun shared(): Int { return 2 }\n"),
        ],
    );
    let err = load_package(&root).unwrap_err();
    assert!(err.contains("duplicate function"), "{err}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_path_deps() {
    let t = parse_aura_toml(
        r#"
[package]
name = "demo.app"

[dependencies]
demo.math = { path = "../math" }
other = "vendor/other"
"#,
    )
    .unwrap();
    assert_eq!(
        t.dependencies.get("demo.math").map(String::as_str),
        Some("../math")
    );
    assert_eq!(
        t.dependencies.get("other").map(String::as_str),
        Some("vendor/other")
    );
}

#[test]
fn lock_parse_and_verify() {
    use super::lock::{parse_lock, verify_lock_against_toml, write_lock};
    use std::collections::HashMap;

    let lock = parse_lock(
        r#"
# comment
demo.math = "../math"
demo.other = "vendor/other"
"#,
    )
    .expect("parse lock");
    assert_eq!(
        lock.packages.get("demo.math").unwrap().path.as_deref(),
        Some("../math")
    );

    let root = std::env::temp_dir().join(format!("aura-lock-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    // C8b: lock paths must exist with aura.toml.
    let math = root.join("math");
    fs::create_dir_all(&math).unwrap();
    fs::write(
        math.join("aura.toml"),
        r#"[package]
name = "demo.math"
"#,
    )
    .unwrap();
    let mut deps = HashMap::new();
    deps.insert("demo.math".into(), "math".into());
    write_lock(&root, &deps).unwrap();
    verify_lock_against_toml(&root, &deps).unwrap();

    let mut bad = deps.clone();
    bad.insert("demo.math".into(), "elsewhere".into());
    let err = verify_lock_against_toml(&root, &bad).unwrap_err();
    assert!(err.contains("aura.lock"), "{err}");

    // Missing path entry fails existence check.
    deps.insert("demo.ghost".into(), "ghost".into());
    write_lock(&root, &deps).unwrap();
    let err = verify_lock_against_toml(&root, &deps).unwrap_err();
    assert!(err.contains("missing") || err.contains("ghost"), "{err}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn lock_parse_registry_schema_c8k() {
    use super::lock::parse_lock;

    let lock = parse_lock(
        r#"
demo.math = "../math"
demo.reg = { version = "1.2.3", checksum = "abc", source = "registry" }
demo.pathv = { path = "../p", version = "0.1.0", source = "path" }
"#,
    )
    .expect("parse mixed lock");
    assert_eq!(
        lock.packages.get("demo.math").unwrap().path.as_deref(),
        Some("../math")
    );
    let reg = lock.packages.get("demo.reg").unwrap();
    assert_eq!(reg.version.as_deref(), Some("1.2.3"));
    assert_eq!(reg.checksum.as_deref(), Some("abc"));
    assert_eq!(reg.source.as_deref(), Some("registry"));
    assert!(reg.path.is_none());
    let pv = lock.packages.get("demo.pathv").unwrap();
    assert_eq!(pv.path.as_deref(), Some("../p"));
    assert_eq!(pv.version.as_deref(), Some("0.1.0"));
}

#[test]
fn load_import_path_dep() {
    let root = std::env::temp_dir().join(format!("aura-pkg-imp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_tree(
        &root,
        &[
            (
                "math/aura.toml",
                r#"[package]
name = "demo.math"
[[bin]]
path = "src"
"#,
            ),
            (
                "math/src/lib.aura",
                r#"package demo.math
pub fun square(x: Int): Int { return x * x }
fun mul(a: Int, b: Int): Int { return a * b }
"#,
            ),
            (
                "app/aura.toml",
                r#"[package]
name = "demo.app"
[[bin]]
name = "app"
path = "src"
[dependencies]
demo.math = { path = "../math" }
"#,
            ),
            (
                "app/src/main.aura",
                r#"package demo.app
import demo.math
fun main() { square(2) }
"#,
            ),
        ],
    );
    let pkg = load_package(&root.join("app/aura.toml")).expect("load app");
    assert_eq!(pkg.package, "demo.app");
    assert!(pkg.sources.len() >= 2, "expected merged sources");
    let names: Vec<_> = pkg
        .ast
        .functions
        .iter()
        .map(|f| f.name.name.as_str())
        .collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"square"));
    assert!(names.contains(&"mul"));
    let square = pkg
        .ast
        .functions
        .iter()
        .find(|f| f.name.name == "square")
        .unwrap();
    assert!(square.is_pub);
    assert_eq!(square.origin_package, "demo.math");
    let _ = fs::remove_dir_all(&root);
}

// --- C13i registry index client ---

fn fixture_registry_index() -> crate::package::RegistryIndex {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/registry");
    crate::package::RegistryIndex::open(&root).expect("open fixture registry")
}

#[test]
fn registry_list_versions_flat_fixture() {
    let idx = fixture_registry_index();
    let vers = idx.list_versions("hello").expect("list hello");
    assert_eq!(vers, vec!["1.0.0", "1.1.0", "1.2.0"]);

    let unyanked = idx.list_versions_unyanked("hello").unwrap();
    assert_eq!(unyanked, vec!["1.0.0", "1.1.0"]);
}

#[test]
fn registry_get_version_meta() {
    let idx = fixture_registry_index();
    let meta = idx.get_version_meta("hello", "1.1.0").unwrap();
    assert_eq!(meta.name, "hello");
    assert_eq!(meta.vers, "1.1.0");
    assert!(meta.cksum.starts_with("sha256:"));
    assert!(!meta.yanked);
    assert_eq!(meta.repository.as_deref(), Some("auraspace/hello"));

    let yanked = idx.get_version_meta("hello", "1.2.0").unwrap();
    assert!(yanked.yanked);

    let err = idx.get_version_meta("hello", "9.9.9").unwrap_err();
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn registry_wrapped_versions_object() {
    let idx = fixture_registry_index();
    let vers = idx.list_versions("demo.http").unwrap();
    assert_eq!(vers, vec!["0.1.0", "0.2.0"]);
    let meta = idx.get_version_meta("demo.http", "0.2.0").unwrap();
    assert_eq!(
        meta.cksum,
        "sha256:2222222222222222222222222222222222222222222222222222222222222222"
    );
}

#[test]
fn registry_sparse_layout_package() {
    let idx = fixture_registry_index();
    // Only present under packages/se/rd/serde/versions.json (RFC-005 sparse)
    let vers = idx.list_versions("serde").unwrap();
    assert_eq!(vers, vec!["1.0.0"]);
    let meta = idx.get_version_meta("serde", "1.0.0").unwrap();
    assert_eq!(meta.repository.as_deref(), Some("auraspace/serde"));
}

#[test]
fn registry_config_loaded() {
    let idx = fixture_registry_index();
    let cfg = idx.config();
    assert!(cfg.dl.as_ref().is_some_and(|d| d.contains("{version}")));
    assert!(cfg.api.as_ref().is_some_and(|a| a.contains("crates-index")));
}

#[test]
fn registry_missing_package() {
    let idx = fixture_registry_index();
    let err = idx.list_versions("no.such.pkg").unwrap_err();
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn registry_default_index_path_shape() {
    use crate::package::{default_index_path, index_root_from_env, ENV_REGISTRY_INDEX};

    let def = default_index_path();
    let s = def.to_string_lossy();
    assert!(
        s.contains(".aura") && s.contains("registry") && s.ends_with("index"),
        "{def:?}"
    );
    // When AURA_REGISTRY_INDEX is unset, env root equals default cache path.
    if std::env::var_os(ENV_REGISTRY_INDEX).is_none() {
        assert_eq!(index_root_from_env(), def);
        // from_env_or_default only succeeds if the cache dir already exists.
        let _ = crate::package::RegistryIndex::from_env_or_default();
    }

    let idx = fixture_registry_index();
    assert!(idx.root().ends_with("testdata/registry") || idx.root().ends_with("testdata\\registry"));
    let _cfg: &crate::package::RegistryConfig = idx.config();
    let _meta: crate::package::VersionMeta = idx.get_version_meta("hello", "1.0.0").unwrap();
}

#[test]
fn registry_open_missing_dir() {
    let err = crate::package::RegistryIndex::open("/no/such/aura/registry/index/xyz").unwrap_err();
    assert!(err.contains("not found"), "{err}");
}

// --- C13j semver caret resolve ---

#[test]
fn semver_resolve_hello_caret_picks_highest_unyanked() {
    let idx = fixture_registry_index();
    // hello: 1.0.0, 1.1.0 unyanked; 1.2.0 yanked
    let meta = super::resolve("hello", "^1.0.0", &idx).unwrap();
    assert_eq!(meta.vers, "1.1.0");
    assert!(!meta.yanked);

    // Bare version == caret
    let bare = super::resolve("hello", "1.0", &idx).unwrap();
    assert_eq!(bare.vers, "1.1.0");

    // Partial "1.1" still caret → >=1.1.0 <2.0.0 → 1.1.0
    let m = super::resolve("hello", "1.1", &idx).unwrap();
    assert_eq!(m.vers, "1.1.0");

    // Exact floor at 1.1.0
    let m = super::resolve("hello", "^1.1.0", &idx).unwrap();
    assert_eq!(m.vers, "1.1.0");
}

#[test]
fn semver_resolve_skips_yanked() {
    let idx = fixture_registry_index();
    // ^1.2.0 would only match 1.2.0 which is yanked
    let err = super::resolve("hello", "^1.2.0", &idx).unwrap_err();
    assert!(err.contains("no matching version"), "{err}");
    assert!(err.contains("hello"), "{err}");
}

#[test]
fn semver_resolve_0x_caret_locks_minor() {
    let idx = fixture_registry_index();
    // demo.http: 0.1.0, 0.2.0
    let m = super::resolve("demo.http", "^0.1.0", &idx).unwrap();
    assert_eq!(m.vers, "0.1.0");

    let m = super::resolve("demo.http", "0.1", &idx).unwrap();
    assert_eq!(m.vers, "0.1.0");

    let m = super::resolve("demo.http", "^0.2.0", &idx).unwrap();
    assert_eq!(m.vers, "0.2.0");

    // `0` / `^0` → >=0.0.0 <1.0.0 → highest 0.2.0
    let m = super::resolve("demo.http", "0", &idx).unwrap();
    assert_eq!(m.vers, "0.2.0");
}

#[test]
fn semver_resolve_sparse_package() {
    let idx = fixture_registry_index();
    let m = super::resolve("serde", "^1", &idx).unwrap();
    assert_eq!(m.vers, "1.0.0");
    assert_eq!(m.repository.as_deref(), Some("auraspace/serde"));
}

#[test]
fn semver_resolve_lock_pin_pure() {
    let idx = fixture_registry_index();
    let (meta, pin) = super::resolve_lock_pin("hello", "^1", &idx).unwrap();
    assert_eq!(meta.vers, "1.1.0");
    assert_eq!(pin.version, "1.1.0");
    assert_eq!(pin.checksum, meta.cksum);
    assert_eq!(pin.source, "registry");

    let line = pin.format_lock_line("hello");
    let lock = super::lock::parse_lock(&line).expect("lock line parses");
    let entry = lock.packages.get("hello").unwrap();
    assert_eq!(entry.version.as_deref(), Some("1.1.0"));
    assert_eq!(entry.checksum.as_deref(), Some(meta.cksum.as_str()));
    assert_eq!(entry.source.as_deref(), Some("registry"));
}

#[test]
fn semver_resolve_via_fixture_index_path() {
    // Uses the same fixture root that AURA_REGISTRY_INDEX would point at in CI.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/registry");
    let idx = super::RegistryIndex::open(&fixture).expect("open via fixture path");
    assert!(
        idx.root().ends_with("testdata/registry") || idx.root().ends_with("testdata\\registry")
    );
    let meta = super::resolve("hello", "1.0.0", &idx).unwrap();
    assert_eq!(meta.vers, "1.1.0");
    // index_root_from_env documents the AURA_REGISTRY_INDEX override contract
    let _ = super::ENV_REGISTRY_INDEX;
}

#[test]
fn semver_parse_rejects_bad_req() {
    assert!(super::parse_req("").is_err());
    assert!(super::parse_req(">=1.0").is_err());
    assert!(super::parse_version("01.0.0").is_err());
    assert!(super::parse_version("1.2.3.4").is_err());
}

#[test]
fn semver_public_reexports() {
    let idx = fixture_registry_index();
    let meta = super::resolve("hello", "^1.0", &idx).unwrap();
    assert_eq!(meta.vers, "1.1.0");
    let pin: super::RegistryLockPin = super::lock_pin_from_meta(&meta);
    assert_eq!(pin.source, "registry");
    let v: super::Version = super::parse_version("1.2.3").unwrap();
    assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
    let req: super::VersionReq = super::parse_req("^1.2.3").unwrap();
    assert!(req.matches(&v));
}
