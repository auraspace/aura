//! Package loading from files, directories, and manifests.

use aura_ast::{shift_file_spans, File, Path as AstPath, Span};
use aura_parser::parse_file;
use std::fs;
use std::path::{Path, PathBuf};

use super::toml::parse_aura_toml;
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

pub(crate) fn load_single_file(path: &Path) -> Result<LoadedPackage, String> {
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
