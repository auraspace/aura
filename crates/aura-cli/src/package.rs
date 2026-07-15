//! Multi-file package loading and minimal `aura.toml` (C3e).

use aura_ast::{shift_file_spans, File, Ident, Path as AstPath, Span};
use aura_parser::{parse_file, ParseError};
use std::fs;
use std::path::{Path, PathBuf};

/// One source file in a package, with its range in the virtual buffer.
#[derive(Debug, Clone)]
pub struct SourceEntry {
    pub path: PathBuf,
    pub src: String,
    /// Inclusive start offset in the virtual concatenated source.
    pub base: u32,
    /// Exclusive end offset in the virtual source.
    pub end: u32,
}

/// Loaded compilation unit: one or more `.aura` files of the same package.
#[derive(Debug, Clone)]
pub struct LoadedPackage {
    /// Directory containing `aura.toml` or the single file's parent.
    pub root: PathBuf,
    /// Dotted package name from Aura sources.
    pub package: String,
    /// Binary name from `aura.toml` `[[bin]].name` or package last segment / file stem.
    pub bin_name: String,
    pub sources: Vec<SourceEntry>,
    /// Virtual concatenation of all sources (for fallback diagnostics).
    pub virtual_src: String,
    /// Merged AST with spans rewritten into `virtual_src`.
    pub ast: File,
}

#[derive(Debug, Clone, Default)]
struct AuraToml {
    package_name: Option<String>,
    bin_name: Option<String>,
    /// Relative path to a source file or directory (default: `src/` or package root).
    bin_path: Option<String>,
}

impl LoadedPackage {
    /// Map a span in the virtual buffer back to a real file path, local source, and local span.
    pub fn locate(&self, span: Span) -> (String, &str, Span) {
        for s in &self.sources {
            if span.start >= s.base && span.start < s.end {
                let local = Span::new(span.start - s.base, span.end.saturating_sub(s.base));
                return (s.path.display().to_string(), s.src.as_str(), local);
            }
        }
        if let Some(s) = self.sources.first() {
            (
                s.path.display().to_string(),
                s.src.as_str(),
                Span::new(0, 0),
            )
        } else {
            ("<unknown>".into(), self.virtual_src.as_str(), span)
        }
    }
}

/// Resolve a CLI path: `.aura` file, directory, or `aura.toml`.
pub fn load_package(path: &Path) -> Result<LoadedPackage, String> {
    if path.is_file() {
        if path.file_name().and_then(|n| n.to_str()) == Some("aura.toml") {
            return load_from_manifest(path);
        }
        if path.extension().and_then(|e| e.to_str()) == Some("aura") {
            return load_single_file(path);
        }
        return Err(format!(
            "error: {}: expected `.aura` file, directory, or `aura.toml`",
            path.display()
        ));
    }
    if path.is_dir() {
        let manifest = path.join("aura.toml");
        if manifest.is_file() {
            return load_from_manifest(&manifest);
        }
        return load_directory(path, None, None);
    }
    Err(format!("error: path not found: {}", path.display()))
}

/// Load from cwd when user passes no path (look for `./aura.toml`).
pub fn load_package_default() -> Result<LoadedPackage, String> {
    let manifest = PathBuf::from("aura.toml");
    if manifest.is_file() {
        return load_from_manifest(&manifest);
    }
    Err(
        "error: no path given and no `aura.toml` in the current directory\n  \
         usage: aura <cmd> <file.aura|dir|aura.toml>"
            .into(),
    )
}

fn load_single_file(path: &Path) -> Result<LoadedPackage, String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let ast = parse_file(&src).map_err(|e| format_parse(path, &src, e))?;
    let package = ast.package.display();
    let bin_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a.out")
        .to_string();
    let end = src.len() as u32;
    Ok(LoadedPackage {
        root: path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        package,
        bin_name,
        sources: vec![SourceEntry {
            path: path.to_path_buf(),
            src: src.clone(),
            base: 0,
            end,
        }],
        virtual_src: src,
        ast,
    })
}

fn load_from_manifest(manifest: &Path) -> Result<LoadedPackage, String> {
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    let toml =
        parse_aura_toml(&text).map_err(|e| format!("error: {}: {e}", manifest.display()))?;
    let root = manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let source_root = match &toml.bin_path {
        Some(p) => root.join(p),
        None => {
            let src_dir = root.join("src");
            if src_dir.is_dir() {
                src_dir
            } else {
                root.clone()
            }
        }
    };

    let mut pkg = if source_root.is_file() {
        load_single_file(&source_root)?
    } else if source_root.is_dir() {
        load_directory(
            &source_root,
            toml.package_name.as_deref(),
            toml.bin_name.as_deref(),
        )?
    } else {
        return Err(format!(
            "error: {}: source path not found: {}",
            manifest.display(),
            source_root.display()
        ));
    };

    pkg.root = root;
    if let Some(name) = toml.package_name {
        if name != pkg.package {
            return Err(format!(
                "error: aura.toml package name `{name}` does not match source package `{}`",
                pkg.package
            ));
        }
    }
    if let Some(bin) = toml.bin_name {
        pkg.bin_name = bin;
    } else if pkg.bin_name.is_empty() || pkg.bin_name == "a.out" {
        pkg.bin_name = last_segment(&pkg.package);
    }
    Ok(pkg)
}

fn load_directory(
    dir: &Path,
    expect_package: Option<&str>,
    bin_name: Option<&str>,
) -> Result<LoadedPackage, String> {
    let mut paths = collect_aura_files(dir)?;
    if paths.is_empty() {
        return Err(format!(
            "error: no `.aura` files found under {}",
            dir.display()
        ));
    }
    paths.sort();

    let mut sources: Vec<SourceEntry> = Vec::new();
    let mut virtual_src = String::new();
    let mut package: Option<String> = None;
    let mut package_path: Option<AstPath> = None;
    let mut interfaces = Vec::new();
    let mut enums = Vec::new();
    let mut classes = Vec::new();
    let mut functions = Vec::new();
    let mut seen_types: Vec<(String, String, String)> = Vec::new(); // kind, name, path
    let mut seen_funs: Vec<(String, String)> = Vec::new(); // name, path

    for path in &paths {
        let src = fs::read_to_string(path)
            .map_err(|e| format!("error: read {}: {e}", path.display()))?;
        let mut ast = parse_file(&src).map_err(|e| format_parse(path, &src, e))?;
        let pkg_name = ast.package.display();
        if let Some(ref p) = package {
            if *p != pkg_name {
                return Err(format!(
                    "error: package mismatch: {} has `package {pkg_name}`, expected `{p}`",
                    path.display()
                ));
            }
        } else {
            package = Some(pkg_name);
        }

        if !virtual_src.is_empty() && !virtual_src.ends_with('\n') {
            virtual_src.push('\n');
        }
        let base = virtual_src.len() as u32;
        shift_file_spans(&mut ast, base);
        virtual_src.push_str(&src);
        if !virtual_src.ends_with('\n') {
            virtual_src.push('\n');
        }
        let end = virtual_src.len() as u32;

        if package_path.is_none() {
            package_path = Some(ast.package.clone());
        }

        for i in &ast.interfaces {
            check_dup_type(&mut seen_types, "interface", &i.name.name, path)?;
        }
        for e in &ast.enums {
            check_dup_type(&mut seen_types, "enum", &e.name.name, path)?;
        }
        for c in &ast.classes {
            let kind = match c.kind {
                aura_ast::NominalKind::Struct => "struct",
                aura_ast::NominalKind::Class => "class",
            };
            check_dup_type(&mut seen_types, kind, &c.name.name, path)?;
        }
        for f in &ast.functions {
            check_dup_fun(&mut seen_funs, &f.name.name, path)?;
        }

        interfaces.extend(ast.interfaces);
        enums.extend(ast.enums);
        classes.extend(ast.classes);
        functions.extend(ast.functions);

        sources.push(SourceEntry {
            path: path.clone(),
            src,
            base,
            end,
        });
    }

    let package = package.unwrap();
    if let Some(expected) = expect_package {
        if expected != package {
            return Err(format!(
                "error: aura.toml package name `{expected}` does not match source package `{package}`"
            ));
        }
    }

    let bin = bin_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| last_segment(&package));

    let pkg_span = sources
        .first()
        .map(|s| Span::new(s.base, s.end))
        .unwrap_or_else(|| Span::new(0, 0));

    let package_path = package_path.unwrap_or_else(|| synthetic_package_path(&package));

    let merged = File {
        package: package_path,
        interfaces,
        enums,
        classes,
        functions,
        span: pkg_span,
    };

    Ok(LoadedPackage {
        root: dir.to_path_buf(),
        package,
        bin_name: bin,
        sources,
        virtual_src,
        ast: merged,
    })
}

fn synthetic_package_path(name: &str) -> AstPath {
    let segments: Vec<Ident> = name
        .split('.')
        .map(|s| Ident {
            name: s.to_string(),
            span: Span::new(0, 0),
        })
        .collect();
    AstPath {
        segments,
        span: Span::new(0, 0),
    }
}

fn last_segment(package: &str) -> String {
    package
        .rsplit('.')
        .next()
        .unwrap_or("a.out")
        .to_string()
}

fn check_dup_type(
    seen: &mut Vec<(String, String, String)>,
    kind: &str,
    name: &str,
    path: &Path,
) -> Result<(), String> {
    if let Some((_, _, prev)) = seen
        .iter()
        .find(|(k, n, _)| k == kind && n == name)
    {
        return Err(format!(
            "error: duplicate {kind} `{name}` in {} (first defined in {prev})",
            path.display()
        ));
    }
    seen.push((
        kind.to_string(),
        name.to_string(),
        path.display().to_string(),
    ));
    Ok(())
}

fn check_dup_fun(seen: &mut Vec<(String, String)>, name: &str, path: &Path) -> Result<(), String> {
    if let Some((_, prev_path)) = seen.iter().find(|(n, _)| n == name) {
        return Err(format!(
            "error: duplicate function `{name}` in {} (first defined in {prev_path})",
            path.display()
        ));
    }
    seen.push((name.to_string(), path.display().to_string()));
    Ok(())
}

fn collect_aura_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_aura_files_rec(dir, &mut out)?;
    Ok(out)
}

fn collect_aura_files_rec(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|e| format!("error: read dir {}: {e}", dir.display()))?;
    for ent in entries {
        let ent = ent.map_err(|e| format!("error: read dir {}: {e}", dir.display()))?;
        let path = ent.path();
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            collect_aura_files_rec(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("aura") {
            out.push(path);
        }
    }
    Ok(())
}

fn format_parse(path: &Path, src: &str, e: ParseError) -> String {
    aura_diagnostics::format_error(&path.display().to_string(), src, &e.message, e.span)
}

/// Minimal TOML subset for C3e (no full TOML dependency).
fn parse_aura_toml(text: &str) -> Result<AuraToml, String> {
    let mut out = AuraToml::default();
    let mut section = String::new();
    let mut in_bin = false;

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if line == "[package]" {
                section = "package".into();
                in_bin = false;
            } else if line == "[[bin]]" {
                section = "bin".into();
                in_bin = true;
            } else {
                section = "other".into();
                in_bin = false;
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(format!("line {}: expected key = value", lineno + 1));
        };
        let key = k.trim();
        let val = parse_toml_string(v.trim())
            .map_err(|e| format!("line {}: {e}", lineno + 1))?;
        match (section.as_str(), key) {
            ("package", "name") => out.package_name = Some(val),
            ("bin", "name") if in_bin => out.bin_name = Some(val),
            ("bin", "path") if in_bin => out.bin_path = Some(val),
            _ => {}
        }
    }
    Ok(out)
}

fn parse_toml_string(v: &str) -> Result<String, String> {
    let v = v.trim();
    if let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(inner.to_string());
    }
    if let Some(inner) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(inner.to_string());
    }
    if v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Ok(v.to_string());
    }
    Err(format!("invalid value `{v}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
        assert_eq!(pkg.sources.len(), 2);
        assert_eq!(pkg.ast.functions.len(), 2);
        let names: Vec<_> = pkg
            .ast
            .functions
            .iter()
            .map(|f| f.name.name.as_str())
            .collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"square"));
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
}
