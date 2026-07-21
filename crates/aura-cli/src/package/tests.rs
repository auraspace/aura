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
