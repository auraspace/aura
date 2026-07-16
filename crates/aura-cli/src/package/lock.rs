//! Minimal `aura.lock` for path dependencies (C3p).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Parsed lock entries: package name → path string (as written relative to package root).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AuraLock {
    pub(crate) packages: BTreeMap<String, String>,
}

pub(crate) fn lock_path(root: &Path) -> std::path::PathBuf {
    root.join("aura.lock")
}

pub(crate) fn read_lock(root: &Path) -> Result<Option<AuraLock>, String> {
    let path = lock_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("error: read {}: {e}", path.display()))?;
    parse_lock(&text).map(Some).map_err(|e| format!("error: {}: {e}", path.display()))
}

pub(crate) fn parse_lock(text: &str) -> Result<AuraLock, String> {
    let mut packages = BTreeMap::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(format!("line {}: expected name = \"path\"", lineno + 1));
        };
        let name = k.trim().to_string();
        let path = parse_quoted(v.trim())
            .map_err(|e| format!("line {}: {e}", lineno + 1))?;
        if packages.insert(name.clone(), path).is_some() {
            return Err(format!("line {}: duplicate package `{name}`", lineno + 1));
        }
    }
    Ok(AuraLock { packages })
}

fn parse_quoted(v: &str) -> Result<String, String> {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return Ok(v[1..v.len() - 1].to_string());
    }
    Err(format!("expected quoted path string, got {v}"))
}

/// Ensure direct `aura.toml` path deps match an existing lockfile (if any).
/// C4j: lock may list extra transitive packages; those are not required in toml.
pub(crate) fn verify_lock_against_toml(
    root: &Path,
    toml_deps: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let Some(lock) = read_lock(root)? else {
        return Ok(());
    };
    for (name, path) in toml_deps {
        match lock.packages.get(name) {
            None => {
                return Err(format!(
                    "error: aura.lock missing package `{name}` (declared in aura.toml)\n  \
                     hint: delete aura.lock and re-run, or add `{name} = \"{path}\"` to aura.lock"
                ));
            }
            Some(locked) if locked != path => {
                return Err(format!(
                    "error: aura.lock path for `{name}` is `{locked}`, but aura.toml has `{path}`\n  \
                     hint: update aura.toml or aura.lock so they agree"
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Write `aura.lock` from path deps (direct + optional transitive; sorted, stable).
/// No-op when there are no path dependencies (avoids empty lockfiles).
///
/// `direct` is used for comments; `all` is written (must include direct).
pub(crate) fn write_lock(
    root: &Path,
    all_deps: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    write_lock_with_direct(root, all_deps, all_deps)
}

/// Write lock with optional direct-vs-transitive annotation (C4j).
pub(crate) fn write_lock_with_direct(
    root: &Path,
    all_deps: &std::collections::HashMap<String, String>,
    direct: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    if all_deps.is_empty() {
        return Ok(());
    }
    let path = lock_path(root);
    let mut body = String::from(
        "# aura.lock — path dependencies (C3p/C4j)\n\
         # Direct deps match aura.toml; extra entries are transitive path deps.\n",
    );
    let sorted: BTreeMap<_, _> = all_deps.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (name, p) in sorted {
        if direct.contains_key(&name) {
            body.push_str(&format!("{name} = \"{p}\"\n"));
        } else {
            body.push_str(&format!("{name} = \"{p}\"  # transitive\n"));
        }
    }
    // Skip write if identical (avoids dirty mtime).
    if path.is_file() {
        if let Ok(existing) = fs::read_to_string(&path) {
            if existing == body {
                return Ok(());
            }
        }
    }
    fs::write(&path, body).map_err(|e| format!("error: write {}: {e}", path.display()))?;
    Ok(())
}
