//! Minimal `aura.lock` for path dependencies (C3p) + registry schema v0 (C8k/C13l).

use super::semver::{parse_req, parse_version, OriginLockPin};
use super::toml::DepSpec;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// One lock entry: path dep and/or registry pin fields (C8k).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LockEntry {
    /// Relative path for path deps (required for source path / legacy form).
    pub(crate) path: Option<String>,
    /// Semver pin (path docs or registry).
    pub(crate) version: Option<String>,
    /// `path` or `git+...` (default path when only path string).
    pub(crate) source: Option<String>,
    /// sha256 for downloaded origins.
    pub(crate) checksum: Option<String>,
    /// Immutable Git commit SHA for VCS origins.
    pub(crate) rev: Option<String>,
}

impl LockEntry {
    pub(crate) fn path_str(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

/// Parsed lock entries: package name → entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AuraLock {
    pub(crate) packages: BTreeMap<String, LockEntry>,
    pub(crate) macro_plugins: BTreeMap<String, LockEntry>,
}

impl AuraLock {
    /// Path map for callers that only need path deps (C3p/C4j).
    #[allow(dead_code)]
    pub(crate) fn path_map(&self) -> BTreeMap<String, String> {
        self.packages
            .iter()
            .filter_map(|(k, e)| e.path.clone().map(|p| (k.clone(), p)))
            .collect()
    }
}

/// One entry to write into `aura.lock` (path string or registry pin).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LockWriteEntry {
    Path {
        path: String,
        /// Transitive path deps get a comment marker.
        transitive: bool,
    },
    Origin(OriginLockPin),
    MacroPlugin {
        path: String,
        checksum: String,
    },
}

pub(crate) fn lock_path(root: &Path) -> std::path::PathBuf {
    root.join("aura.lock")
}

pub(crate) fn read_lock(root: &Path) -> Result<Option<AuraLock>, String> {
    let path = lock_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
    parse_lock(&text)
        .map(Some)
        .map_err(|e| format!("error: {}: {e}", path.display()))
}

pub(crate) fn parse_lock(text: &str) -> Result<AuraLock, String> {
    let mut packages = BTreeMap::new();
    let mut macro_plugins = BTreeMap::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(format!(
                "line {}: expected name = \"path\" or name = {{ … }}",
                lineno + 1
            ));
        };
        let name = k.trim().to_string();
        let entry = parse_lock_value(v.trim()).map_err(|e| format!("line {}: {e}", lineno + 1))?;
        if let Some(plugin_name) = name.strip_prefix("macro_plugin.") {
            if plugin_name.is_empty() {
                return Err(format!("line {}: empty macro plugin name", lineno + 1));
            }
            if macro_plugins
                .insert(plugin_name.to_string(), entry)
                .is_some()
            {
                return Err(format!(
                    "line {}: duplicate macro plugin `{plugin_name}`",
                    lineno + 1
                ));
            }
        } else if packages.insert(name.clone(), entry).is_some() {
            return Err(format!("line {}: duplicate package `{name}`", lineno + 1));
        }
    }
    Ok(AuraLock {
        packages,
        macro_plugins,
    })
}

fn parse_lock_value(v: &str) -> Result<LockEntry, String> {
    let v = v.trim();
    if v.starts_with('{') {
        return parse_inline_table(v);
    }
    let path = parse_quoted(v)?;
    Ok(LockEntry {
        path: Some(path),
        version: None,
        source: Some("path".into()),
        checksum: None,
        rev: None,
    })
}

/// Minimal TOML-like inline table: `{ key = "val", … }`.
fn parse_inline_table(v: &str) -> Result<LockEntry, String> {
    let v = v.trim();
    if !v.starts_with('{') || !v.ends_with('}') {
        return Err(format!("expected inline table, got {v}"));
    }
    let inner = v[1..v.len() - 1].trim();
    let mut entry = LockEntry {
        source: Some("path".into()),
        ..Default::default()
    };
    if inner.is_empty() {
        return Err("empty lock entry table".into());
    }
    for part in split_top_level_commas(inner) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((k, val)) = part.split_once('=') else {
            return Err(format!("expected key = value in table, got `{part}`"));
        };
        let key = k.trim();
        let val = parse_quoted(val.trim())?;
        match key {
            "path" => entry.path = Some(val),
            "version" => entry.version = Some(val),
            "source" => entry.source = Some(val),
            "checksum" => entry.checksum = Some(val),
            "rev" => entry.rev = Some(val),
            other => {
                return Err(format!(
                    "unknown lock field `{other}` (expected path, version, source, checksum)"
                ));
            }
        }
    }
    // Git entries need immutable version/checksum/revision; plugin entries need
    // path/checksum; path entries need path.
    let source = entry.source.as_deref().unwrap_or("path");
    if source == "plugin" {
        if entry.path.is_none() {
            return Err("macro plugin lock entry requires path".into());
        }
        if entry.checksum.is_none() {
            return Err("macro plugin lock entry requires checksum".into());
        }
    } else if source.starts_with("git+") {
        if entry.version.is_none() || entry.checksum.is_none() || entry.rev.is_none() {
            return Err("Git lock entry requires version, checksum, and rev".into());
        }
        let rev = entry.rev.as_deref().unwrap_or_default();
        if rev.len() != 40 || !rev.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Git lock entry rev must be a 40-character commit SHA".into());
        }
    } else if source == "path" {
        if entry.path.is_none() {
            return Err("path lock entry requires path".into());
        }
    } else if source == "registry" {
        return Err("legacy registry lock entries are unsupported; use a Git origin".into());
    } else if entry.path.is_none() {
        return Err("path lock entry requires path".into());
    }
    Ok(entry)
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    // No nested tables; just split on commas (values are quoted, no commas inside).
    s.split(',').collect()
}

fn parse_quoted(v: &str) -> Result<String, String> {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return Ok(v[1..v.len() - 1].to_string());
    }
    Err(format!("expected quoted string, got {v}"))
}

/// Ensure direct `aura.toml` deps match an existing lockfile (if any).
/// C4j: lock may list extra transitive packages; those are not required in toml.
/// C8b: every locked path (direct + transitive) must resolve under the package root.
pub(crate) fn verify_lock_against_toml(
    root: &Path,
    toml_deps: &std::collections::HashMap<String, DepSpec>,
) -> Result<(), String> {
    let Some(lock) = read_lock(root)? else {
        return Ok(());
    };
    for (name, dep) in toml_deps {
        match lock.packages.get(name) {
            None => {
                return Err(format!(
                    "error: aura.lock missing package `{name}` (declared in aura.toml)\n  \
                     hint: delete aura.lock and re-run, or add `{name}` to aura.lock"
                ));
            }
            Some(entry) => match dep {
                DepSpec::Path(path) => {
                    if entry
                        .source
                        .as_deref()
                        .is_some_and(|source| source != "path")
                    {
                        return Err(format!(
                            "error: aura.lock has registry pin for `{name}`, but aura.toml declares a path dependency\n  \
                             hint: update aura.toml or delete aura.lock and re-run"
                        ));
                    }
                    let locked = entry.path_str().unwrap_or("");
                    if locked != path {
                        return Err(format!(
                            "error: aura.lock path for `{name}` is `{locked}`, but aura.toml has `{path}`\n  \
                             hint: update aura.toml or aura.lock so they agree"
                        ));
                    }
                }
                DepSpec::Git {
                    source,
                    subdir,
                    version,
                    tag,
                    rev,
                } => {
                    let source_identity = subdir
                        .as_deref()
                        .map(|path| format!("{source}#subdir={path}"))
                        .unwrap_or_else(|| source.clone());
                    let expected_source =
                        format!("git+{}", super::origin::canonical_source(&source_identity)?);
                    if entry.source.as_deref() != Some(expected_source.as_str()) {
                        return Err(format!(
                            "error: aura.lock source for `{name}` does not match Git origin `{source}`"
                        ));
                    }
                    let Some(locked_rev) = entry.rev.as_deref() else {
                        return Err(format!(
                            "error: aura.lock Git dependency `{name}` is missing rev"
                        ));
                    };
                    if let Some(expected_rev) = rev {
                        if locked_rev != expected_rev {
                            return Err(format!(
                                "error: aura.lock rev for `{name}` is `{locked_rev}`, but aura.toml pins `{expected_rev}`"
                            ));
                        }
                    }
                    if let Some(requirement) = version {
                        let requirement = parse_req(requirement).map_err(|e| {
                            format!("error: package `{name}`: invalid version requirement `{requirement}`: {e}")
                        })?;
                        let locked_version = entry.version.as_deref().ok_or_else(|| {
                            format!("error: aura.lock Git dependency `{name}` is missing version")
                        })?;
                        let pinned = parse_version(locked_version).map_err(|e| {
                            format!("error: aura.lock package `{name}`: invalid version `{locked_version}`: {e}")
                        })?;
                        if !requirement.matches(&pinned) {
                            return Err(format!(
                                "error: aura.lock version for `{name}` is `{locked_version}`, which does not satisfy `{}`",
                                requirement.raw
                            ));
                        }
                    }
                    if let Some(expected_tag) = tag {
                        let expected_version =
                            expected_tag.strip_prefix('v').unwrap_or(expected_tag);
                        if entry.version.as_deref() != Some(expected_version) {
                            return Err(format!(
                                "error: aura.lock version for `{name}` does not match tag `{expected_tag}`"
                            ));
                        }
                    }
                }
            },
        }
    }
    // Path entries must exist on disk; Git pins are satisfied by origin resolution.
    for (name, entry) in &lock.packages {
        let source = entry.source.as_deref().unwrap_or("path");
        if source.starts_with("git+") {
            continue;
        }
        let Some(rel) = entry.path_str() else {
            return Err(format!(
                "error: aura.lock package `{name}` has no path (source={source})"
            ));
        };
        let dep_path = root.join(rel);
        if !dep_path.is_dir() {
            return Err(format!(
                "error: aura.lock package `{name}` path `{rel}` is missing or not a directory\n  \
                 hint: fix the path, restore the dependency, or delete aura.lock and re-run"
            ));
        }
        if !dep_path.join("aura.toml").is_file() {
            return Err(format!(
                "error: aura.lock package `{name}` path `{rel}` has no aura.toml\n  \
                 hint: point at a package root that contains aura.toml"
            ));
        }
    }
    Ok(())
}

/// Write `aura.lock` from path deps only (tests / simple callers).
/// No-op when there are no dependencies.
#[allow(dead_code)]
pub(crate) fn write_lock(
    root: &Path,
    all_deps: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    write_lock_with_direct(root, all_deps, all_deps)
}

/// Write lock with optional direct-vs-transitive annotation (C4j) — path deps only.
pub(crate) fn write_lock_with_direct(
    root: &Path,
    all_deps: &std::collections::HashMap<String, String>,
    direct: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let mut entries = BTreeMap::new();
    for (name, p) in all_deps {
        entries.insert(
            name.clone(),
            LockWriteEntry::Path {
                path: p.clone(),
                transitive: !direct.contains_key(name),
            },
        );
    }
    write_lock_entries(root, &entries)
}

/// Write path + immutable origin lock entries. Sorted by package name.
pub(crate) fn write_lock_entries(
    root: &Path,
    entries: &BTreeMap<String, LockWriteEntry>,
) -> Result<(), String> {
    let path = lock_path(root);
    if entries.is_empty() {
        if path.is_file() {
            fs::remove_file(&path).map_err(|e| format!("error: remove {}: {e}", path.display()))?;
        }
        return Ok(());
    }
    let mut body = String::from(
        "# aura.lock — path dependencies and immutable Git pins\n\
         # Direct deps match aura.toml; extra path entries are transitive.\n\
         # Git pins: name = { version = \"…\", checksum = \"…\", source = \"git+…\", rev = \"…\" }\n\
         # Macro pins: macro_plugin.Name = { path = \"…\", checksum = \"…\", source = \"plugin\" }\n",
    );
    for (name, entry) in entries {
        match entry {
            LockWriteEntry::Path {
                path: p,
                transitive,
            } => {
                if *transitive {
                    body.push_str(&format!("{name} = \"{p}\"  # transitive\n"));
                } else {
                    body.push_str(&format!("{name} = \"{p}\"\n"));
                }
            }
            LockWriteEntry::Origin(pin) => {
                body.push_str(&pin.format_lock_line(name));
                body.push('\n');
            }
            LockWriteEntry::MacroPlugin { path: p, checksum } => {
                body.push_str(&format!(
                    "macro_plugin.{name} = {{ path = \"{p}\", checksum = \"{checksum}\", source = \"plugin\" }}\n"
                ));
            }
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
