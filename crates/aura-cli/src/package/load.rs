//! Package loading from files, directories, and manifests (C3e/C3f).

use aura_ast::{shift_file_spans, File, ImportDecl, Path as AstPath, Span};
use aura_parser::parse_file;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::lock::{verify_lock_against_toml, write_lock};
use super::toml::{parse_aura_toml, AuraToml};
use super::types::{LoadedPackage, SourceEntry};
use super::util::{
    check_dup_fun, check_dup_type, collect_aura_files, format_parse, last_segment,
    synthetic_package_path,
};

/// Resolve a CLI path: `.aura` file, directory, or `aura.toml`.
pub fn load_package(path: &Path) -> Result<LoadedPackage, String> {
    if path.is_file() {
        if path.file_name().and_then(|n| n.to_str()) == Some("aura.toml") {
            return load_from_manifest(path);
        }
        if path.extension().and_then(|e| e.to_str()) == Some("aura") {
            return load_single_file_entry(path);
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
        let pkg = load_directory(path, None, None)?;
        if !pkg.ast.imports.is_empty() {
            return Err(format!(
                "error: {}: `import` requires an `aura.toml` with [dependencies] path entries",
                path.display()
            ));
        }
        return Ok(pkg);
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

/// CLI entry for a lone `.aura` file: if it has `import`s, prefer nearby `aura.toml`.
fn load_single_file_entry(path: &Path) -> Result<LoadedPackage, String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let ast = parse_file(&src).map_err(|e| format_parse(path, &src, e))?;
    if !ast.imports.is_empty() {
        if let Some(parent) = path.parent() {
            let manifest = parent.join("aura.toml");
            if manifest.is_file() {
                return load_from_manifest(&manifest);
            }
            if let Some(grand) = parent.parent() {
                let m2 = grand.join("aura.toml");
                if m2.is_file() {
                    return load_from_manifest(&m2);
                }
            }
        }
        return Err(format!(
            "error: {}: `import` requires an `aura.toml` with [dependencies] path entries",
            path.display()
        ));
    }
    load_single_file(path)
}

pub(crate) fn load_single_file(path: &Path) -> Result<LoadedPackage, String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let mut ast = parse_file(&src).map_err(|e| format_parse(path, &src, e))?;
    let package = ast.package.display();
    stamp_origin(&mut ast, &package);
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

pub(crate) fn load_from_manifest(manifest: &Path) -> Result<LoadedPackage, String> {
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

    pkg.root = root.clone();
    if let Some(ref name) = toml.package_name {
        if name != &pkg.package {
            return Err(format!(
                "error: aura.toml package name `{name}` does not match source package `{}`",
                pkg.package
            ));
        }
    }
    if let Some(bin) = toml.bin_name.clone() {
        pkg.bin_name = bin;
    } else if pkg.bin_name.is_empty() || pkg.bin_name == "a.out" {
        pkg.bin_name = last_segment(&pkg.package);
    }

    // C3p: if aura.lock exists, path deps must match it.
    verify_lock_against_toml(&root, &toml.dependencies)?;

    // C4g: auto-prelude — make std.io available and import it for app packages.
    let mut effective = toml.clone();
    apply_std_io_prelude(&mut pkg, &mut effective, &root)?;

    // Merge path deps from this manifest and from each loaded dep's own aura.toml.
    resolve_imports(&mut pkg, &effective, &root)?;

    // Refresh lockfile for this package's direct path deps (path-only, C3p).
    // Do not write auto-prelude entries into lock (only user-declared deps).
    write_lock(&root, &toml.dependencies)?;
    Ok(pkg)
}

/// C4g: resolve `std/io` on disk and inject `import std.io` for non-std packages.
fn apply_std_io_prelude(
    pkg: &mut LoadedPackage,
    toml: &mut AuraToml,
    root: &Path,
) -> Result<(), String> {
    // Never prelude the std packages themselves.
    if pkg.package == "std.io" || pkg.package.starts_with("std.") {
        return Ok(());
    }
    let std_io = match find_std_io_dir(root) {
        Some(p) => p,
        None => return Ok(()), // silent skip if std not discoverable
    };
    if !toml.dependencies.contains_key("std.io") {
        // Prefer absolute path so nested packages resolve reliably.
        toml.dependencies
            .insert("std.io".into(), std_io.display().to_string());
    }
    let already = pkg
        .ast
        .imports
        .iter()
        .any(|i| i.path.display() == "std.io");
    if !already {
        pkg.ast.imports.push(ImportDecl {
            path: AstPath {
                segments: vec![
                    aura_ast::Ident {
                        name: "std".into(),
                        span: Span::new(0, 0),
                    },
                    aura_ast::Ident {
                        name: "io".into(),
                        span: Span::new(0, 0),
                    },
                ],
                span: Span::new(0, 0),
            },
            alias: None,
            origin_package: pkg.package.clone(),
            span: Span::new(0, 0),
        });
    }
    let _ = std_io;
    Ok(())
}

/// Walk up from `from` (and env `AURA_STD`) looking for `std/<leaf>` package dir.
fn find_std_package_dir(from: &Path, leaf: &str) -> Option<PathBuf> {
    if let Ok(std_root) = std::env::var("AURA_STD") {
        let p = PathBuf::from(std_root).join(leaf);
        if p.is_dir() && p.join("aura.toml").is_file() {
            return fs::canonicalize(&p).ok().or(Some(p));
        }
    }
    let start = fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf());
    let mut cur = Some(start.as_path());
    while let Some(dir) = cur {
        let candidate = dir.join("std").join(leaf);
        if candidate.is_dir() && candidate.join("aura.toml").is_file() {
            return fs::canonicalize(&candidate).ok().or(Some(candidate));
        }
        cur = dir.parent();
    }
    None
}

fn find_std_io_dir(from: &Path) -> Option<PathBuf> {
    find_std_package_dir(from, "io")
}



/// Load path dependencies for `import` and merge their ASTs into the unit.
fn resolve_imports(
    pkg: &mut LoadedPackage,
    toml: &AuraToml,
    root: &Path,
) -> Result<(), String> {
    let mut pending: Vec<String> = pkg
        .ast
        .imports
        .iter()
        .map(|i| i.path.display())
        .collect();
    let mut loaded: HashSet<String> = HashSet::new();
    loaded.insert(pkg.package.clone());

    // deps map: package name → absolute path (grows as nested manifests are read)
    let mut deps: HashMap<String, PathBuf> = toml
        .dependencies
        .iter()
        .map(|(k, p)| (k.clone(), root.join(p)))
        .collect();

    // C4h: auto-resolve std.* path when imported but not declared (assert, etc.).
    for imp in pending.iter() {
        if deps.contains_key(imp) {
            continue;
        }
        if let Some(leaf) = imp.strip_prefix("std.") {
            if let Some(p) = find_std_package_dir(root, leaf) {
                deps.insert(imp.clone(), p);
            }
        }
    }

    while let Some(imp) = pending.pop() {
        if !loaded.insert(imp.clone()) {
            continue;
        }
        let dep_path = deps.get(&imp).cloned().ok_or_else(|| {
            format!(
                "error: package `{}` imports `{imp}` but no path dependency is declared in aura.toml\n  \
                 hint: add `{imp} = {{ path = \"...\" }}` under [dependencies]",
                pkg.package
            )
        })?;
        let dep_pkg = load_dep_package(&dep_path)?;
        if dep_pkg.package != imp {
            return Err(format!(
                "error: dependency `{imp}` at {} has package name `{}`",
                dep_path.display(),
                dep_pkg.package
            ));
        }
        // Merge nested path deps relative to the dependency package root.
        if let Ok(nested) = read_manifest_deps(&dep_pkg.root) {
            for (k, p) in nested {
                deps.entry(k).or_insert_with(|| dep_pkg.root.join(p));
            }
        }
        for i in &dep_pkg.ast.imports {
            let name = i.path.display();
            if !loaded.contains(&name) {
                pending.push(name);
            }
        }
        merge_package(pkg, dep_pkg)?;
    }
    Ok(())
}

fn read_manifest_deps(root: &Path) -> Result<HashMap<String, String>, String> {
    let manifest = root.join("aura.toml");
    if !manifest.is_file() {
        return Ok(HashMap::new());
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    let toml = parse_aura_toml(&text).map_err(|e| format!("error: {}: {e}", manifest.display()))?;
    Ok(toml.dependencies)
}

fn load_dep_package(path: &Path) -> Result<LoadedPackage, String> {
    if path.join("aura.toml").is_file() {
        // Load sources only — do not re-enter resolve_imports (root owns the graph).
        return load_package_sources_only(path);
    }
    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("aura") {
        return load_single_file(path);
    }
    if path.is_dir() {
        return load_directory(path, None, None);
    }
    Err(format!(
        "error: dependency path not found: {}",
        path.display()
    ))
}

/// Load a package directory/manifest without resolving its imports (used for deps).
fn load_package_sources_only(root: &Path) -> Result<LoadedPackage, String> {
    let manifest = root.join("aura.toml");
    if manifest.is_file() {
        let text = fs::read_to_string(&manifest)
            .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
        let toml =
            parse_aura_toml(&text).map_err(|e| format!("error: {}: {e}", manifest.display()))?;
        let source_root = match &toml.bin_path {
            Some(p) => root.join(p),
            None => {
                let src_dir = root.join("src");
                if src_dir.is_dir() {
                    src_dir
                } else {
                    root.to_path_buf()
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
        pkg.root = root.to_path_buf();
        if let Some(name) = toml.package_name {
            if name != pkg.package {
                return Err(format!(
                    "error: aura.toml package name `{name}` does not match source package `{}`",
                    pkg.package
                ));
            }
        }
        return Ok(pkg);
    }
    load_directory(root, None, None)
}

fn merge_package(into: &mut LoadedPackage, mut dep: LoadedPackage) -> Result<(), String> {
    // Append sources into virtual buffer with span shift.
    if !into.virtual_src.is_empty() && !into.virtual_src.ends_with('\n') {
        into.virtual_src.push('\n');
    }
    let base = into.virtual_src.len() as u32;
    shift_file_spans(&mut dep.ast, base);

    // Shift each source entry base/end.
    for s in &mut dep.sources {
        s.base += base;
        s.end += base;
    }
    into.virtual_src.push_str(&dep.virtual_src);
    if !into.virtual_src.ends_with('\n') {
        into.virtual_src.push('\n');
    }
    // Fix end of last dep source after possible trailing newline pad... keep as shifted.

    // Duplicate checks against existing unit.
    let mut seen_types: Vec<(String, String, String)> = Vec::new();
    let mut seen_funs: Vec<(String, String)> = Vec::new();
    for i in &into.ast.interfaces {
        seen_types.push((
            "interface".into(),
            i.name.name.clone(),
            i.origin_package.clone(),
        ));
    }
    for e in &into.ast.enums {
        seen_types.push(("enum".into(), e.name.name.clone(), e.origin_package.clone()));
    }
    for c in &into.ast.classes {
        let kind = match c.kind {
            aura_ast::NominalKind::Struct => "struct",
            aura_ast::NominalKind::Class => "class",
        };
        seen_types.push((kind.into(), c.name.name.clone(), c.origin_package.clone()));
    }
    for f in &into.ast.functions {
        seen_funs.push((f.name.name.clone(), f.origin_package.clone()));
    }

    for i in &dep.ast.interfaces {
        // C4d: same simple name allowed across packages (C symbols package-prefixed).
        if seen_types.iter().any(|(k, n, p)| {
            k == "interface" && n == &i.name.name && p == &i.origin_package
        }) {
            return Err(format!(
                "error: duplicate interface `{}` when linking package `{}`",
                i.name.name, dep.package
            ));
        }
    }
    for e in &dep.ast.enums {
        // C3v: same simple name allowed across packages (C symbols are package-prefixed).
        if seen_types
            .iter()
            .any(|(k, n, p)| k == "enum" && n == &e.name.name && p == &e.origin_package)
        {
            return Err(format!(
                "error: duplicate enum `{}` when linking package `{}`",
                e.name.name, dep.package
            ));
        }
    }
    for c in &dep.ast.classes {
        let kind = match c.kind {
            aura_ast::NominalKind::Struct => "struct",
            aura_ast::NominalKind::Class => "class",
        };
        // C3v: same simple name allowed across packages.
        if seen_types
            .iter()
            .any(|(k, n, p)| k == kind && n == &c.name.name && p == &c.origin_package)
        {
            return Err(format!(
                "error: duplicate {kind} `{}` when linking package `{}`",
                c.name.name, dep.package
            ));
        }
    }
    for f in &dep.ast.functions {
        // C3o: same simple name allowed across packages (C symbols are package-prefixed).
        if seen_funs
            .iter()
            .any(|(n, p)| n == &f.name.name && p == &f.origin_package)
        {
            return Err(format!(
                "error: duplicate function `{}` when linking package `{}`",
                f.name.name, dep.package
            ));
        }
    }

    into.ast.imports.extend(dep.ast.imports);
    into.ast.interfaces.extend(dep.ast.interfaces);
    into.ast.enums.extend(dep.ast.enums);
    into.ast.classes.extend(dep.ast.classes);
    into.ast.functions.extend(dep.ast.functions);
    into.sources.extend(dep.sources);
    Ok(())
}

fn stamp_origin(ast: &mut File, package: &str) {
    for imp in &mut ast.imports {
        if imp.origin_package.is_empty() {
            imp.origin_package = package.to_string();
        }
    }
    for i in &mut ast.interfaces {
        if i.origin_package.is_empty() {
            i.origin_package = package.to_string();
        }
    }
    for e in &mut ast.enums {
        if e.origin_package.is_empty() {
            e.origin_package = package.to_string();
        }
    }
    for c in &mut ast.classes {
        if c.origin_package.is_empty() {
            c.origin_package = package.to_string();
        }
        for m in &mut c.methods {
            if m.origin_package.is_empty() {
                m.origin_package = package.to_string();
            }
        }
    }
    for f in &mut ast.functions {
        if f.origin_package.is_empty() {
            f.origin_package = package.to_string();
        }
    }
}

pub(crate) fn load_directory(
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
    let mut imports = Vec::new();
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
            package = Some(pkg_name.clone());
        }

        stamp_origin(&mut ast, &pkg_name);

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

        imports.extend(ast.imports);
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
        imports,
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
