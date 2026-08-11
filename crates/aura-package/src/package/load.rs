//! Package loading from files, directories, and manifests (C3e/C3f/C13l).

use aura_analysis::{
    declarative_macro_names, declarative_macro_sources, parse_file, parse_file_with_macro_sources,
};
use aura_ast::{shift_file_spans, File, ImportDecl, Path as AstPath, Span};
use aura_codegen::Profile;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::archive::{archive_sha256, build_source_archive};
use super::fetch::{cache_root_from_env, install_from_bytes, package_src_dir, sha256_hex};
use super::lock::{
    read_lock, verify_lock_against_toml, write_lock_entries, AuraLock, LockWriteEntry,
};
use super::origin::{resolve_git, OriginResolution};
use super::registry::VersionMeta;
use super::semver::OriginLockPin;
use super::toml::{native_config_for_target, parse_aura_toml, AuraToml, DepSpec};
use super::types::{LoadedPackage, SourceEntry};
use super::util::{
    check_dup_fun, check_dup_type, collect_aura_files, format_parse, last_segment,
    synthetic_package_path,
};

/// Resolve a CLI path: `.aura` file, directory, or `aura.toml`.
pub fn load_package(path: &Path) -> Result<LoadedPackage, String> {
    load_package_with_lock(path, true, None)
}

/// Parse a manifest without touching the filesystem or resolving dependencies.
/// This is intentionally small so malformed input can be fuzzed safely.
pub fn parse_manifest_for_fuzz(text: &str) -> Result<(), String> {
    parse_aura_toml(text).map(|_| ())
}

/// Return the canonical member manifests of a workspace root.
pub fn workspace_members(path: &Path) -> Result<Vec<PathBuf>, String> {
    let manifest = graph_manifest(path)?;
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("error: read {}: {error}", manifest.display()))?;
    let toml = parse_aura_toml(&text)
        .map_err(|error| format!("error: {}: {error}", manifest.display()))?;
    if toml.workspace_members.is_empty() {
        return Err(format!(
            "error: {} is not a workspace root",
            manifest.display()
        ));
    }
    let root = manifest_root(&manifest);
    toml.workspace_members
        .into_iter()
        .map(|member| {
            let member_path = root.join(member);
            graph_manifest(&member_path)
        })
        .collect()
}

/// Load every workspace member with the normal lockfile policy.
pub fn load_workspace(path: &Path) -> Result<Vec<LoadedPackage>, String> {
    workspace_members(path)?
        .into_iter()
        .map(|manifest| load_package(&manifest))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyNode {
    pub name: String,
    pub source: Option<String>,
    pub dependencies: Vec<DependencyNode>,
}

/// Read the package dependency graph without mutating manifests or lockfiles.
/// Path dependencies are expanded recursively; immutable VCS origins remain
/// leaves because their source tree is owned by the package cache.
pub fn dependency_graph(path: &Path) -> Result<DependencyNode, String> {
    let manifest = graph_manifest(path)?;
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("error: read {}: {error}", manifest.display()))?;
    let toml = parse_aura_toml(&text)
        .map_err(|error| format!("error: {}: {error}", manifest.display()))?;
    if toml.package_name.is_none() && !toml.workspace_members.is_empty() {
        let root = manifest_root(&manifest);
        let mut dependencies = Vec::new();
        for member in toml.workspace_members {
            let child = graph_manifest(&root.join(member))?;
            let mut node = graph_node(&child, &mut Vec::new())?;
            node.source = Some(format!("workspace:{}", child.display()));
            dependencies.push(node);
        }
        return Ok(DependencyNode {
            name: root.display().to_string(),
            source: Some("workspace".into()),
            dependencies,
        });
    }
    graph_node(&manifest, &mut Vec::new())
}

fn graph_manifest(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        if path.file_name().and_then(|name| name.to_str()) == Some("aura.toml") {
            return Ok(path.to_path_buf());
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("aura") {
            return path
                .parent()
                .map(|parent| parent.join("aura.toml"))
                .filter(|manifest| manifest.is_file())
                .ok_or_else(|| format!("error: no aura.toml next to {}", path.display()));
        }
    }
    let manifest = if path.is_dir() {
        path.join("aura.toml")
    } else {
        path.to_path_buf()
    };
    manifest
        .is_file()
        .then_some(manifest)
        .ok_or_else(|| format!("error: package manifest not found: {}", path.display()))
}

fn graph_node(manifest: &Path, active: &mut Vec<PathBuf>) -> Result<DependencyNode, String> {
    let canonical = fs::canonicalize(manifest).unwrap_or_else(|_| manifest.to_path_buf());
    if active.contains(&canonical) {
        let cycle = active
            .iter()
            .chain(std::iter::once(&canonical))
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(format!("error: dependency cycle detected: {cycle}"));
    }
    active.push(canonical);
    let text = fs::read_to_string(manifest)
        .map_err(|error| format!("error: read {}: {error}", manifest.display()))?;
    let toml = parse_aura_toml(&text)
        .map_err(|error| format!("error: {}: {error}", manifest.display()))?;
    let root = manifest_root(manifest);
    let name = toml
        .package_name
        .clone()
        .unwrap_or_else(|| root.display().to_string());
    let mut dependencies = Vec::new();
    let mut entries = toml.dependencies.into_iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (dependency_name, spec) in entries {
        match spec {
            DepSpec::Path(relative) => {
                let child = graph_manifest(&root.join(relative))?;
                let mut node = graph_node(&child, active)?;
                node.name = dependency_name;
                node.source = Some(format!("path:{}", child.display()));
                dependencies.push(node);
            }
            DepSpec::Git { source, .. } => dependencies.push(DependencyNode {
                name: dependency_name,
                source: Some(format!("git:{source}")),
                dependencies: Vec::new(),
            }),
        }
    }
    active.pop();
    Ok(DependencyNode {
        name,
        source: None,
        dependencies,
    })
}

/// Load and resolve a package without updating its lockfile.
///
/// Language-server requests must not mutate a workspace merely to compute
/// diagnostics, but they need the same resolved dependency graph as the CLI.
pub fn load_package_read_only(path: &Path) -> Result<LoadedPackage, String> {
    load_package_with_lock(path, false, None)
}

/// Load and resolve a package against one specific standard-library root.
///
/// The language server uses this when launched through an Aura toolchain so
/// diagnostics match the std sources shipped with that exact toolchain.
pub fn load_package_read_only_with_std(
    path: &Path,
    std_root: &Path,
) -> Result<LoadedPackage, String> {
    load_package_with_lock(path, false, Some(std_root))
}

fn load_package_with_lock(
    path: &Path,
    write_lock: bool,
    std_root: Option<&Path>,
) -> Result<LoadedPackage, String> {
    if path.is_file() {
        if path.file_name().and_then(|n| n.to_str()) == Some("aura.toml") {
            return load_from_manifest(path, write_lock, std_root);
        }
        if path.extension().and_then(|e| e.to_str()) == Some("aura") {
            return load_single_file_entry(path, write_lock, std_root);
        }
        return Err(format!(
            "error: {}: expected `.aura` file, directory, or `aura.toml`",
            path.display()
        ));
    }
    if path.is_dir() {
        let manifest = path.join("aura.toml");
        if manifest.is_file() {
            return load_from_manifest(&manifest, write_lock, std_root);
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
        return load_from_manifest(&manifest, true, None);
    }
    Err(
        "error: no path given and no `aura.toml` in the current directory\n  \
         usage: aura <cmd> <file.aura|dir|aura.toml>"
            .into(),
    )
}

/// CLI entry for a lone `.aura` file: if it has `import`s, prefer nearby `aura.toml`.
fn load_single_file_entry(
    path: &Path,
    write_lock: bool,
    std_root: Option<&Path>,
) -> Result<LoadedPackage, String> {
    let src =
        fs::read_to_string(path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let ast = parse_file(&src).map_err(|e| format_parse(path, &src, e))?;
    if !ast.imports.is_empty() {
        if let Some(parent) = path.parent() {
            let manifest = parent.join("aura.toml");
            if manifest.is_file() {
                return load_from_manifest(&manifest, write_lock, std_root);
            }
            if let Some(grand) = parent.parent() {
                let m2 = grand.join("aura.toml");
                if m2.is_file() {
                    return load_from_manifest(&m2, write_lock, std_root);
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
    let src =
        fs::read_to_string(path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let mut ast = parse_file(&src).map_err(|e| format_parse(path, &src, e))?;
    let package = ast.package.display();
    stamp_origin(&mut ast, &package);
    let bin_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a.out")
        .to_string();
    let end = src.len() as u32;
    let macro_sources = declarative_macro_sources(&src).unwrap_or_default();
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
        macro_sources,
        macro_plugins: std::collections::BTreeMap::new(),
        native: BTreeMap::new(),
        native_roots: BTreeMap::new(),
        profile_settings: aura_codegen::ProfileSettings::for_profile(Profile::Dev),
    })
}

pub(crate) fn load_from_manifest(
    manifest: &Path,
    write_lock: bool,
    std_root: Option<&Path>,
) -> Result<LoadedPackage, String> {
    let text = fs::read_to_string(manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    let toml = parse_aura_toml(&text).map_err(|e| format!("error: {}: {e}", manifest.display()))?;
    let root = manifest_root(manifest);
    let target = std::env::var("TARGET").unwrap_or_else(|_| "native".into());

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
    pkg.profile_settings = toml.profiles[&Profile::Dev].clone();
    pkg.native = native_config_for_target(&toml, &target);
    pkg.native_roots = pkg
        .native
        .keys()
        .map(|name| (name.clone(), root.clone()))
        .collect();
    for (name, path) in &toml.macro_plugins {
        let plugin_path = Path::new(path);
        if plugin_path.is_absolute()
            || plugin_path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!(
                "error: root macro plugin `{name}` must use a package-relative path without `..`: {path}"
            ));
        }
    }
    pkg.macro_plugins = toml
        .macro_plugins
        .iter()
        .map(|(name, path)| (name.clone(), root.join(path)))
        .collect();
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

    // If aura.lock exists, direct deps must match it.
    verify_lock_against_toml(&root, &toml.dependencies)?;
    let macro_plugin_entries = macro_plugin_lock_entries(&root, &toml.macro_plugins)?;
    verify_macro_plugin_lock(&root, &toml.macro_plugins, write_lock)?;

    // C4g: auto-prelude — make std.io available and import it for app packages.
    let mut effective = toml.clone();
    apply_std_io_prelude(&mut pkg, &mut effective, &root, std_root)?;

    let mut registry = OriginResolver::new(&root)?;
    materialize_origin_deps_with(&mut effective, &toml, &mut registry)?;

    // Merge path deps from this manifest and from each loaded dep's own aura.toml.
    // C4j: also collect the full resolved path map for aura.lock.
    let resolved = resolve_imports(&mut pkg, &effective, &root, std_root, &mut registry)?;

    // Refresh lockfile: path deps + immutable origin pins.
    // Exclude auto-prelude-only entries not declared in the user's aura.toml.
    let mut lock_entries: BTreeMap<String, LockWriteEntry> = BTreeMap::new();
    for (name, pin) in &registry.pins {
        lock_entries.insert(name.clone(), LockWriteEntry::Origin(pin.clone()));
    }
    for (name, abs) in &resolved {
        if registry.pins.contains_key(name) {
            continue;
        }
        if let Some(DepSpec::Path(rel)) = toml.dependencies.get(name) {
            lock_entries.insert(
                name.clone(),
                LockWriteEntry::Path {
                    path: rel.clone(),
                    transitive: false,
                },
            );
        } else if name.starts_with("std.") {
            // Auto std path resolve — omit from lock (not user-declared).
            continue;
        } else if !toml.dependencies.contains_key(name) {
            // Transitive path: store path relative to this package root when possible.
            let rel = path_for_lock(&root, abs);
            lock_entries.insert(
                name.clone(),
                LockWriteEntry::Path {
                    path: rel,
                    transitive: true,
                },
            );
        }
    }
    for (name, entry) in &macro_plugin_entries {
        lock_entries.insert(name.clone(), entry.clone());
    }
    if write_lock {
        write_lock_entries(&root, &lock_entries)?;
    }
    Ok(pkg)
}

fn manifest_root(manifest: &Path) -> PathBuf {
    manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve direct origins and rewrite them as absolute path deps on `effective`.
fn materialize_origin_deps_with(
    effective: &mut AuraToml,
    original: &AuraToml,
    registry: &mut OriginResolver,
) -> Result<(), String> {
    let mut registry_names: Vec<(String, DepSpec)> = original
        .dependencies
        .iter()
        .filter_map(|(n, d)| match d {
            DepSpec::Git { .. } => Some((n.clone(), d.clone())),
            DepSpec::Path(_) => None,
        })
        .collect();
    registry_names.sort_by(|a, b| a.0.cmp(&b.0));
    if registry_names.is_empty() {
        return Ok(());
    }

    for (name, dep) in registry_names {
        let (pin, installed) = match &dep {
            DepSpec::Git { .. } => {
                let (meta, pin, resolution) = registry.resolve_git(&name, &dep)?;
                let installed = ensure_origin_src(&meta, &resolution, &registry.cache)?;
                (pin, installed)
            }
            DepSpec::Path(_) => unreachable!(),
        };
        effective
            .dependencies
            .insert(name.clone(), DepSpec::Path(installed.display().to_string()));
        registry.pins.insert(name, pin);
    }
    Ok(())
}

struct OriginResolver {
    lock: Option<AuraLock>,
    cache: PathBuf,
    pins: BTreeMap<String, OriginLockPin>,
}

fn macro_plugin_lock_entries(
    root: &Path,
    plugins: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, LockWriteEntry>, String> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|e| format!("error: canonicalize package root {}: {e}", root.display()))?;
    let mut entries = BTreeMap::new();
    for (name, relative) in plugins {
        let path = root.join(relative);
        let canonical = fs::canonicalize(&path).map_err(|e| {
            format!("error: root macro plugin `{name}` is unavailable at `{relative}`: {e}")
        })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(format!(
                "error: root macro plugin `{name}` resolves outside the package root: {relative}"
            ));
        }
        let bytes = fs::read(&canonical)
            .map_err(|e| format!("error: read root macro plugin `{name}` at `{relative}`: {e}"))?;
        entries.insert(
            name.clone(),
            LockWriteEntry::MacroPlugin {
                path: relative.clone(),
                checksum: sha256_hex(&bytes),
            },
        );
    }
    Ok(entries)
}

fn verify_macro_plugin_lock(
    root: &Path,
    plugins: &BTreeMap<String, String>,
    write_lock: bool,
) -> Result<(), String> {
    if plugins.is_empty() {
        return Ok(());
    }
    let Some(lock) = read_lock(root)? else {
        if write_lock {
            return Ok(());
        }
        return Err(
            "error: aura.lock is required for root procedural macro plugins\n  hint: run `aura check` or `aura build` once to create plugin checksum pins"
                .into(),
        );
    };
    for (name, relative) in plugins {
        let Some(entry) = lock.macro_plugins.get(name) else {
            if write_lock {
                continue;
            }
            return Err(format!(
                "error: aura.lock missing checksum pin for root macro plugin `{name}`\n  hint: run `aura check` or `aura build` to refresh the lockfile"
            ));
        };
        if entry.source.as_deref() != Some("plugin") || entry.path.as_deref() != Some(relative) {
            return Err(format!(
                "error: aura.lock pin for root macro plugin `{name}` does not match path `{relative}`\n  hint: refresh aura.lock after changing the plugin manifest"
            ));
        }
        let expected = entry.checksum.as_deref().unwrap_or("").to_ascii_lowercase();
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "error: aura.lock checksum for root macro plugin `{name}` is invalid"
            ));
        }
        let current =
            macro_plugin_lock_entries(root, &BTreeMap::from([(name.clone(), relative.clone())]))?;
        let LockWriteEntry::MacroPlugin { checksum, .. } = current
            .get(name)
            .expect("current macro plugin entry must exist")
        else {
            unreachable!();
        };
        if expected != checksum.to_ascii_lowercase() {
            return Err(format!(
                "error: root macro plugin `{name}` checksum mismatch\n  expected {expected}\n  got {checksum}\n  hint: inspect the executable, then refresh aura.lock intentionally"
            ));
        }
    }
    Ok(())
}

impl OriginResolver {
    fn new(root: &Path) -> Result<Self, String> {
        Ok(Self {
            lock: read_lock(root)?,
            cache: cache_root_from_env(),
            pins: BTreeMap::new(),
        })
    }

    fn resolve_git(
        &mut self,
        name: &str,
        dep: &DepSpec,
    ) -> Result<(VersionMeta, OriginLockPin, OriginResolution), String> {
        if let Some(pin) = self.pins.get(name) {
            let source = pin
                .source
                .strip_prefix("git+")
                .ok_or_else(|| format!("error: conflicting origins for `{name}`"))?;
            let locked = DepSpec::Git {
                source: source.into(),
                subdir: None,
                version: Some(pin.version.clone()),
                tag: None,
                rev: pin.rev.clone(),
            };
            let resolution = resolve_git(name, &locked)?;
            return self.git_result(name, resolution);
        }
        let lock_pin = self.lock.as_ref().and_then(|lock| lock.packages.get(name));
        if self.lock.is_some() && lock_pin.is_none() {
            return Err(format!(
                "error: aura.lock missing package `{name}` required by a Git dependency"
            ));
        }
        if let Some(entry) = lock_pin {
            let source = entry
                .source
                .as_deref()
                .and_then(|source| source.strip_prefix("git+"))
                .ok_or_else(|| {
                    format!("error: aura.lock has a non-Git entry for Git dependency `{name}`")
                })?;
            let locked_version = entry.version.as_deref().ok_or_else(|| {
                format!("error: aura.lock Git dependency `{name}` is missing version")
            })?;
            let locked_rev = entry.rev.as_deref().ok_or_else(|| {
                format!("error: aura.lock Git dependency `{name}` is missing rev")
            })?;
            let locked_checksum = normalize_checksum(entry.checksum.as_deref().unwrap_or(""));
            if origin_cache_is_valid(&self.cache, name, locked_version, &locked_checksum) {
                return self.git_result(
                    name,
                    OriginResolution {
                        source: source.into(),
                        version: locked_version.into(),
                        rev: locked_rev.into(),
                        checksum: locked_checksum,
                        archive: Vec::new(),
                    },
                );
            }
            let locked = DepSpec::Git {
                source: source.into(),
                subdir: None,
                version: Some(locked_version.into()),
                tag: None,
                rev: Some(locked_rev.into()),
            };
            let resolution = resolve_git(name, &locked)?;
            let expected = entry.checksum.as_deref().unwrap_or("");
            if normalize_checksum(expected) != resolution.checksum {
                return Err(format!(
                    "error: Git dependency `{name}` checksum mismatch in aura.lock\n  expected {expected}\n  got sha256:{}",
                    resolution.checksum
                ));
            }
            return self.git_result(name, resolution);
        }
        let resolution = resolve_git(name, dep)?;
        self.git_result(name, resolution)
    }

    fn git_result(
        &self,
        name: &str,
        resolution: OriginResolution,
    ) -> Result<(VersionMeta, OriginLockPin, OriginResolution), String> {
        let checksum = format!("sha256:{}", resolution.checksum);
        let meta = VersionMeta {
            name: name.into(),
            vers: resolution.version.clone(),
            cksum: checksum.clone(),
            yanked: false,
            repository: Some(resolution.source.clone()),
            targets: None,
            min_aura: None,
            max_aura: None,
            revoked: false,
            revoke_reason: None,
        };
        let pin = OriginLockPin {
            version: resolution.version.clone(),
            checksum,
            source: format!("git+{}", resolution.source),
            rev: Some(resolution.rev.clone()),
        };
        Ok((meta, pin, resolution))
    }
}

fn normalize_checksum(checksum: &str) -> String {
    checksum
        .strip_prefix("sha256:")
        .unwrap_or(checksum)
        .to_ascii_lowercase()
}

fn origin_cache_is_valid(cache: &Path, name: &str, version: &str, checksum: &str) -> bool {
    package_src_dir(cache, name, version)
        .join("aura.toml")
        .is_file()
        && fs::read_to_string(
            cache
                .join("checksums")
                .join(format!("{name}-{version}.sha256")),
        )
        .map(|value| value.trim().eq_ignore_ascii_case(checksum))
        .unwrap_or(false)
}

fn ensure_origin_src(
    meta: &VersionMeta,
    resolution: &OriginResolution,
    cache: &Path,
) -> Result<PathBuf, String> {
    let dest = package_src_dir(cache, &meta.name, &meta.vers);
    let expected = normalize_checksum(&meta.cksum);
    let marker = cache
        .join("checksums")
        .join(format!("{}-{}.sha256", meta.name, meta.vers));
    if dest.is_dir()
        && dest.join("aura.toml").is_file()
        && fs::read_to_string(&marker)
            .map(|value| value.trim().eq_ignore_ascii_case(&expected))
            .unwrap_or(false)
        && cached_source_matches(&dest, &meta.name, &meta.vers, &expected)
    {
        return Ok(dest);
    }
    if dest.exists() {
        fs::remove_dir_all(&dest)
            .map_err(|e| format!("error: replace cached package `{}`: {e}", meta.name))?;
    }
    if resolution.archive.is_empty() {
        return Err(format!(
            "error: cached Git dependency `{}` failed checksum validation and its origin is unavailable",
            meta.name
        ));
    }
    let installed = install_from_bytes(meta, &resolution.archive, Some(cache))?;
    fs::create_dir_all(marker.parent().expect("checksum marker has parent"))
        .map_err(|e| format!("error: create registry checksum cache: {e}"))?;
    fs::write(&marker, expected)
        .map_err(|e| format!("error: write registry checksum marker: {e}"))?;
    Ok(installed)
}

fn cached_source_matches(dest: &Path, name: &str, version: &str, expected: &str) -> bool {
    let mut files = Vec::new();
    if collect_cached_files(dest, &mut files).is_err() {
        return false;
    }
    let entries = files
        .into_iter()
        .filter_map(|path| {
            let relative = path
                .strip_prefix(dest)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((relative, fs::read(path).ok()?))
        })
        .collect::<Vec<_>>();
    build_source_archive(name, version, &entries)
        .map(|archive| archive_sha256(&archive).eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn collect_cached_files(current: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut children = fs::read_dir(current)
        .map_err(|e| format!("error: read cache directory {}: {e}", current.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    children.sort();
    for child in children {
        if child.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        if child.is_dir() {
            collect_cached_files(&child, out)?;
        } else if child.is_file() {
            out.push(child);
        }
    }
    Ok(())
}

/// Prefer a relative path for lock entries when `abs` is under or near `root`.
fn path_for_lock(root: &Path, abs: &Path) -> String {
    if let (Ok(r), Ok(a)) = (fs::canonicalize(root), fs::canonicalize(abs)) {
        if let Ok(rel) = a.strip_prefix(&r) {
            return rel.display().to_string();
        }
        // Walk up from root to find a relative path with `../`.
        let mut prefix = r.as_path();
        let mut ups = String::new();
        loop {
            if let Ok(rel) = a.strip_prefix(prefix) {
                return format!("{ups}{}", rel.display());
            }
            match prefix.parent() {
                Some(p) if p != prefix => {
                    ups.push_str("../");
                    prefix = p;
                }
                _ => break,
            }
        }
    }
    abs.display().to_string()
}

/// C4g: resolve `std/io` on disk and inject `import std.io` for non-std packages.
fn apply_std_io_prelude(
    pkg: &mut LoadedPackage,
    toml: &mut AuraToml,
    root: &Path,
    std_root: Option<&Path>,
) -> Result<(), String> {
    // Never prelude the std packages themselves.
    if pkg.package == "std.io" || pkg.package.starts_with("std.") {
        return Ok(());
    }
    let std_io = match find_std_package_dir(root, "io", std_root) {
        Some(p) => p,
        None => return Ok(()), // silent skip if std not discoverable
    };
    if !toml.dependencies.contains_key("std.io") {
        // Prefer absolute path so nested packages resolve reliably.
        toml.dependencies
            .insert("std.io".into(), DepSpec::Path(std_io.display().to_string()));
    }
    let already = pkg.ast.imports.iter().any(|i| i.path.display() == "std.io");
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

fn find_std_package_dir(from: &Path, leaf: &str, std_root: Option<&Path>) -> Option<PathBuf> {
    if let Some(std_root) = std_root {
        let package = std_root.join(leaf);
        return package
            .join("aura.toml")
            .is_file()
            .then(|| fs::canonicalize(&package).unwrap_or(package));
    }
    crate::std_path::find_std_package_dir(from, leaf)
}

/// Load path dependencies for `import` and merge their ASTs into the unit.
/// Returns the resolved package name → absolute path map (C4j).
fn resolve_imports(
    pkg: &mut LoadedPackage,
    toml: &AuraToml,
    root: &Path,
    std_root: Option<&Path>,
    registry: &mut OriginResolver,
) -> Result<HashMap<String, PathBuf>, String> {
    let mut loaded = HashSet::new();
    loaded.insert(pkg.package.clone());
    let mut deps: HashMap<String, PathBuf> = toml
        .dependencies
        .iter()
        .filter_map(|(k, d)| d.as_path().map(|p| (k.clone(), root.join(p))))
        .collect();

    let root_package = pkg.package.clone();
    visit_imports(
        pkg,
        root,
        &mut deps,
        &mut loaded,
        &mut vec![root_package],
        std_root,
        registry,
    )?;
    Ok(deps)
}

fn visit_imports(
    pkg: &mut LoadedPackage,
    root: &Path,
    deps: &mut HashMap<String, PathBuf>,
    loaded: &mut HashSet<String>,
    active: &mut Vec<String>,
    std_root: Option<&Path>,
    registry: &mut OriginResolver,
) -> Result<(), String> {
    let mut imports: Vec<String> = pkg.ast.imports.iter().map(|i| i.path.display()).collect();
    imports.sort();
    for imp in imports {
        if active.iter().any(|name| name == &imp) {
            let mut cycle = active.clone();
            cycle.push(imp.clone());
            return Err(format!(
                "error: dependency cycle detected: {}",
                cycle.join(" -> ")
            ));
        }
        if loaded.contains(&imp) {
            continue;
        }
        if !deps.contains_key(&imp) {
            if let Some(leaf) = imp.strip_prefix("std.") {
                if let Some(path) = find_std_package_dir(root, leaf, std_root) {
                    deps.insert(imp.clone(), path);
                }
            }
        }
        let dep_path = deps.get(&imp).cloned().ok_or_else(|| {
            if imp.starts_with("std.") {
                format!(
                    "error: package `{}` imports `{imp}` but the standard library was not found\n  \
                     hint: reinstall Aura (release tarball includes share/aura/std) or set AURA_STD to the monorepo `std/` directory\n  \
                     hint: or add `{imp} = {{ path = \"...\" }}` under [dependencies]",
                    pkg.package
                )
            } else {
                format!(
                    "error: package `{}` imports `{imp}` but no path dependency is declared in aura.toml\n  \
                     hint: add `{imp} = {{ path = \"...\" }}` under [dependencies]",
                    pkg.package
                )
            }
        })?;
        active.push(imp.clone());
        let mut dep_pkg = load_dep_package(&dep_path)?;
        if dep_pkg.package != imp {
            return Err(format!(
                "error: dependency `{imp}` at {} has package name `{}`",
                dep_path.display(),
                dep_pkg.package
            ));
        }
        loaded.insert(imp.clone());

        let dep_toml = read_manifest(&dep_pkg.root)?;
        let mut effective = dep_toml.clone();
        materialize_origin_deps_with(&mut effective, &dep_toml, registry)?;
        let mut nested_deps: HashMap<String, PathBuf> = effective
            .dependencies
            .iter()
            .filter_map(|(name, dep)| {
                dep.as_path()
                    .map(|path| (name.clone(), dep_pkg.root.join(path)))
            })
            .collect();
        for (name, path) in &nested_deps {
            deps.entry(name.clone()).or_insert_with(|| path.clone());
        }
        let dep_root = dep_pkg.root.clone();
        visit_imports(
            &mut dep_pkg,
            &dep_root,
            &mut nested_deps,
            loaded,
            active,
            std_root,
            registry,
        )?;
        active.pop();
        merge_package(pkg, dep_pkg)?;
    }
    Ok(())
}

fn read_manifest(root: &Path) -> Result<AuraToml, String> {
    let manifest = root.join("aura.toml");
    if !manifest.is_file() {
        return Ok(AuraToml::default());
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    parse_aura_toml(&text).map_err(|e| format!("error: {}: {e}", manifest.display()))
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
        if !toml.macro_plugins.is_empty() {
            let names = toml
                .macro_plugins
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "error: dependency package `{}` declares procedural macro plugin(s) `{names}`; dependency plugins are not executable implicitly\n  hint: declare and pin plugins in the root package `[macro_plugins]` table",
                toml.package_name.as_deref().unwrap_or("<unnamed>")
            ));
        }
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
        let target = std::env::var("TARGET").unwrap_or_else(|_| "native".into());
        pkg.native = native_config_for_target(&toml, &target);
        pkg.native_roots = pkg
            .native
            .keys()
            .map(|name| (name.clone(), root.to_path_buf()))
            .collect();
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
    for (name, config) in &dep.native {
        if into.native.contains_key(name) {
            return Err(format!(
                "error: duplicate native library `{name}` in resolved package graph"
            ));
        }
        into.native.insert(name.clone(), config.clone());
        into.native_roots.insert(
            name.clone(),
            dep.native_roots
                .get(name)
                .cloned()
                .unwrap_or_else(|| dep.root.clone()),
        );
    }
    let mut existing = HashSet::new();
    for source in &into.macro_sources {
        existing.extend(
            declarative_macro_names(source)
                .map_err(|error| format!("error: invalid exported macro: {}", error.message))?,
        );
    }
    for source in &dep.macro_sources {
        for name in declarative_macro_names(source)
            .map_err(|error| format!("error: invalid exported macro: {}", error.message))?
        {
            if !existing.insert(name.clone()) {
                return Err(format!(
                    "error: duplicate declarative macro `{name}` in resolved package graph; macro names must be unique"
                ));
            }
        }
    }
    into.macro_sources.extend(dep.macro_sources.iter().cloned());
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
    for f in &into.ast.async_functions {
        seen_funs.push((f.name.name.clone(), f.origin_package.clone()));
    }

    for i in &dep.ast.interfaces {
        // C4d: same simple name allowed across packages (C symbols package-prefixed).
        if seen_types
            .iter()
            .any(|(k, n, p)| k == "interface" && n == &i.name.name && p == &i.origin_package)
        {
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
    for f in &dep.ast.async_functions {
        if seen_funs
            .iter()
            .any(|(n, p)| n == &f.name.name && p == &f.origin_package)
        {
            return Err(format!(
                "error: duplicate async function `{}` when linking package `{}`",
                f.name.name, dep.package
            ));
        }
    }

    into.ast.imports.extend(dep.ast.imports);
    into.ast.interfaces.extend(dep.ast.interfaces);
    into.ast.enums.extend(dep.ast.enums);
    into.ast.classes.extend(dep.ast.classes);
    // Type aliases participate in dependency public APIs just like nominal
    // types. Keep their source package so sema can resolve imported aliases.
    into.ast.type_aliases.extend(dep.ast.type_aliases);
    into.ast.functions.extend(dep.ast.functions);
    into.ast.foreign_functions.extend(dep.ast.foreign_functions);
    into.ast.async_functions.extend(dep.ast.async_functions);
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
    for t in &mut ast.type_aliases {
        if t.origin_package.is_empty() {
            t.origin_package = package.to_string();
        }
    }
    for c in &mut ast.consts {
        if c.origin_package.is_empty() {
            c.origin_package = package.to_string();
        }
    }
    for f in &mut ast.functions {
        if f.origin_package.is_empty() {
            f.origin_package = package.to_string();
        }
    }
    for f in &mut ast.foreign_functions {
        if f.origin_package.is_empty() {
            f.origin_package = package.to_string();
        }
    }
    for f in &mut ast.async_functions {
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
    let mut type_aliases = Vec::new();
    let mut consts = Vec::new();
    let mut interfaces = Vec::new();
    let mut enums = Vec::new();
    let mut classes = Vec::new();
    let mut functions = Vec::new();
    let mut foreign_functions = Vec::new();
    let mut async_functions = Vec::new();
    let mut macro_sources = Vec::new();
    let mut seen_macros: HashSet<String> = HashSet::new();
    let mut seen_types: Vec<(String, String, String)> = Vec::new(); // kind, name, path
    let mut seen_funs: Vec<(String, String)> = Vec::new(); // name, path

    for path in &paths {
        let src =
            fs::read_to_string(path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
        let mut ast = parse_file_with_macro_sources(&src, &macro_sources)
            .map_err(|e| format_parse(path, &src, e))?;
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
        for t in &ast.type_aliases {
            check_dup_type(&mut seen_types, "type", &t.name.name, path)?;
        }
        for c in &ast.consts {
            check_dup_fun(&mut seen_funs, &c.name.name, path)?;
        }
        for f in &ast.functions {
            check_dup_fun(&mut seen_funs, &f.name.name, path)?;
        }
        for f in &ast.foreign_functions {
            check_dup_fun(&mut seen_funs, &f.name.name, path)?;
        }
        for f in &ast.async_functions {
            check_dup_fun(&mut seen_funs, &f.name.name, path)?;
        }

        imports.extend(ast.imports);
        interfaces.extend(ast.interfaces);
        enums.extend(ast.enums);
        classes.extend(ast.classes);
        type_aliases.extend(ast.type_aliases);
        consts.extend(ast.consts);
        functions.extend(ast.functions);
        foreign_functions.extend(ast.foreign_functions);
        async_functions.extend(ast.async_functions);

        let exported = declarative_macro_sources(&src).map_err(|e| format_parse(path, &src, e))?;
        for name in exported {
            for macro_name in
                declarative_macro_names(&name).map_err(|e| format_parse(path, &src, e))?
            {
                if !seen_macros.insert(macro_name.clone()) {
                    return Err(format!(
                        "error: duplicate declarative macro `{macro_name}` in package source `{}`; macro names must be unique",
                        path.display()
                    ));
                }
            }
            macro_sources.push(name);
        }

        sources.push(SourceEntry {
            path: path.clone(),
            src,
            base,
            end,
        });
    }

    let package = package.ok_or_else(|| {
        format!(
            "error: no package declaration found under {}",
            dir.display()
        )
    })?;
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
        type_aliases,
        consts,
        functions,
        foreign_functions,
        async_functions,
        span: pkg_span,
    };

    Ok(LoadedPackage {
        root: dir.to_path_buf(),
        package,
        bin_name: bin,
        sources,
        virtual_src,
        ast: merged,
        macro_sources,
        macro_plugins: std::collections::BTreeMap::new(),
        native: BTreeMap::new(),
        native_roots: BTreeMap::new(),
        profile_settings: aura_codegen::ProfileSettings::for_profile(Profile::Dev),
    })
}

#[cfg(test)]
mod tests {
    use super::{dependency_graph, manifest_root};
    use std::fs;
    use std::path::Path;

    #[test]
    fn relative_manifest_without_parent_uses_current_directory() {
        assert_eq!(manifest_root(Path::new("aura.toml")), Path::new("."));
    }

    #[test]
    fn dependency_graph_expands_nested_path_packages() {
        let root = std::env::temp_dir().join(format!("aura-graph-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("app")).unwrap();
        fs::create_dir_all(root.join("mid")).unwrap();
        fs::create_dir_all(root.join("leaf")).unwrap();
        fs::write(
            root.join("app/aura.toml"),
            "[package]\nname = \"app\"\n[dependencies]\nmid = { path = \"../mid\" }\n",
        )
        .unwrap();
        fs::write(
            root.join("mid/aura.toml"),
            "[package]\nname = \"mid\"\n[dependencies]\nleaf = { path = \"../leaf\" }\n",
        )
        .unwrap();
        fs::write(root.join("leaf/aura.toml"), "[package]\nname = \"leaf\"\n").unwrap();

        let graph = dependency_graph(&root.join("app")).unwrap();
        assert_eq!(graph.name, "app");
        assert_eq!(graph.dependencies[0].name, "mid");
        assert_eq!(graph.dependencies[0].dependencies[0].name, "leaf");
        let _ = fs::remove_dir_all(&root);
    }
}
