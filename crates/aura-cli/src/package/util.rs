//! Path and file-collection helpers.

use aura_ast::{Ident, Path as AstPath, Span};
use aura_parser::ParseError;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn synthetic_package_path(name: &str) -> AstPath {
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

pub(crate) fn last_segment(package: &str) -> String {
    package
        .rsplit('.')
        .next()
        .unwrap_or("a.out")
        .to_string()
}

pub(crate) fn check_dup_type(
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

pub(crate) fn check_dup_fun(seen: &mut Vec<(String, String)>, name: &str, path: &Path) -> Result<(), String> {
    if let Some((_, prev_path)) = seen.iter().find(|(n, _)| n == name) {
        return Err(format!(
            "error: duplicate function `{name}` in {} (first defined in {prev_path})",
            path.display()
        ));
    }
    seen.push((name.to_string(), path.display().to_string()));
    Ok(())
}

pub(crate) fn collect_aura_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_aura_files_rec(dir, &mut out)?;
    Ok(out)
}

pub(crate) fn collect_aura_files_rec(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
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

pub(crate) fn format_parse(path: &Path, src: &str, e: ParseError) -> String {
    aura_diagnostics::format_error(&path.display().to_string(), src, &e.message, e.span)
}
