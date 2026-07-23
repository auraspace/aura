//! Read-only package publication preview (U4).
//!
//! This module deliberately does not call [`super::load_package`]: package loading
//! may refresh `aura.lock`, while a dry-run must not mutate the package or registry.

use super::archive::{archive_sha256, build_source_archive};
use super::lock::read_lock;
use super::registry::{publish_upload, PublishError, PublishReceipt};
use super::semver::{parse_req, parse_version};
use super::toml::{parse_aura_toml, AuraToml, DepSpec};
use aura_parser::parse_file;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SOURCE_FILES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// The bounded result of `aura publish --dry-run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPreview {
    pub package: String,
    pub version: String,
    pub archive_name: String,
    pub archive: Vec<u8>,
    pub checksum: String,
    /// Signing is intentionally explicit until the release signing primitive exists.
    pub signature: Option<String>,
    pub source_entries: Vec<String>,
    pub dependency_count: usize,
}

impl PublishPreview {
    pub fn render(&self) -> String {
        let signature = self
            .signature
            .as_deref()
            .unwrap_or("deferred (no signing key or signing primitive configured)");
        format!(
            "dry-run publish\npackage {} {}\narchive {} ({} bytes)\nsha256 {}\nsignature {}\nsource entries {}\ndependencies {}\nregistry mutation none",
            self.package,
            self.version,
            self.archive_name,
            self.archive.len(),
            self.checksum,
            signature,
            self.source_entries.len(),
            self.dependency_count,
        )
    }
}

/// Validate and preview publication for a manifest or package directory.
///
/// This function is read-only: it does not resolve/fetch registry dependencies,
/// write a lockfile, contact a registry, or write an archive. Registry
/// dependencies must have a valid local lock pin so the preview remains bounded
/// and deterministic.
pub fn publish_dry_run(path: impl AsRef<Path>) -> Result<PublishPreview, String> {
    let manifest = manifest_path(path.as_ref())?;
    let root = manifest
        .parent()
        .ok_or_else(|| format!("error: manifest has no parent: {}", manifest.display()))?;
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("error: read {}: {e}", manifest.display()))?;
    let toml = parse_aura_toml(&text).map_err(|e| format!("error: {}: {e}", manifest.display()))?;

    let mut errors = Vec::new();
    let package = match toml.package_name.as_deref() {
        Some(name) if valid_package_name(name) => name.to_string(),
        Some(name) => {
            errors.push(format!(
                "error: manifest package name is unsafe or empty: `{name}`"
            ));
            name.to_string()
        }
        None => {
            errors.push("error: manifest is missing `[package].name`".into());
            String::new()
        }
    };
    let version = match toml.package_version.as_deref() {
        Some(version) => match validate_release_version(version) {
            Ok(()) => version.to_string(),
            Err(error) => {
                errors.push(error);
                version.to_string()
            }
        },
        None => {
            errors.push("error: manifest is missing `[package].version`".into());
            String::new()
        }
    };

    let source_root = source_root(root, &toml);
    let entries = collect_source_entries(root, &source_root, &manifest, &mut errors);
    validate_dependencies(root, &toml, &mut errors);

    if !errors.is_empty() {
        return Err(format_errors(&errors));
    }

    let archive = build_source_archive(&package, &version, &entries)
        .map_err(|e| format!("error: package archive: {e}"))?;
    let checksum = archive_sha256(&archive);
    let source_entries = entries.into_iter().map(|(path, _)| path).collect();
    Ok(PublishPreview {
        package: package.clone(),
        version: version.clone(),
        archive_name: format!("{package}-{version}.crate"),
        archive,
        checksum,
        signature: None,
        source_entries,
        dependency_count: toml.dependencies.len(),
    })
}

/// Validate locally, then perform the single bounded registry upload.
pub fn publish_package(
    path: impl AsRef<Path>,
    registry_url: &str,
    token: Option<&str>,
) -> Result<PublishReceipt, PublishError> {
    let preview = publish_dry_run(path).map_err(|message| PublishError {
        kind: super::registry::PublishErrorKind::Rejected,
        status: Some(400),
        attempts: 0,
        message,
    })?;
    publish_upload(registry_url, token, &preview)
}

fn manifest_path(path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_dir() {
        path.join("aura.toml")
    } else {
        path.to_path_buf()
    };
    if candidate.file_name().and_then(|name| name.to_str()) != Some("aura.toml") {
        return Err(format!(
            "error: publish dry-run requires an `aura.toml` manifest, got {}",
            candidate.display()
        ));
    }
    if !candidate.is_file() {
        return Err(format!(
            "error: manifest not found: {}",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn source_root(root: &Path, toml: &AuraToml) -> PathBuf {
    match &toml.bin_path {
        Some(path) => root.join(path),
        None if root.join("src").is_dir() => root.join("src"),
        None => root.to_path_buf(),
    }
}

fn collect_source_entries(
    root: &Path,
    source_root: &Path,
    manifest: &Path,
    errors: &mut Vec<String>,
) -> Vec<(String, Vec<u8>)> {
    let mut paths = Vec::new();
    let root_real = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            errors.push(format!("error: package root is unreadable: {error}"));
            return Vec::new();
        }
    };
    let source_real = match fs::canonicalize(source_root) {
        Ok(path) if path.starts_with(&root_real) => path,
        Ok(path) => {
            errors.push(format!(
                "error: source path {} escapes package root {}",
                path.display(),
                root_real.display()
            ));
            return Vec::new();
        }
        Err(error) => {
            errors.push(format!(
                "error: source path {} is unreadable: {error}",
                source_root.display()
            ));
            return Vec::new();
        }
    };
    collect_aura_paths(&source_real, &root_real, &mut paths, errors);
    paths.sort();

    let mut entries = Vec::with_capacity(paths.len() + 1);
    match fs::read(manifest) {
        Ok(bytes) => entries.push(("aura.toml".to_string(), bytes)),
        Err(error) => errors.push(format!("error: read manifest: {error}")),
    }
    let mut total = entries.first().map(|(_, bytes)| bytes.len()).unwrap_or(0);
    for path in paths {
        if entries.len() >= MAX_SOURCE_FILES {
            errors.push(format!(
                "error: package contains more than {MAX_SOURCE_FILES} source entries"
            ));
            break;
        }
        let relative = match path.strip_prefix(&root_real) {
            Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                errors.push(format!(
                    "error: source path escapes package root: {}",
                    path.display()
                ));
                continue;
            }
        };
        match fs::read(&path) {
            Ok(bytes) => {
                total = total.saturating_add(bytes.len());
                if total > MAX_SOURCE_BYTES {
                    errors.push(format!(
                        "error: package sources exceed {MAX_SOURCE_BYTES} bytes"
                    ));
                    break;
                }
                if let Err(error) = parse_source(&path, &bytes) {
                    errors.push(error);
                }
                entries.push((relative, bytes));
            }
            Err(error) => errors.push(format!("error: read source {}: {error}", path.display())),
        }
    }
    if entries.len() == 1 {
        errors.push("error: package contains no `.aura` source entries".into());
    }
    entries
}

fn collect_aura_paths(dir: &Path, root: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    let read_dir = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!(
                "error: read source directory {}: {error}",
                dir.display()
            ));
            return;
        }
    };
    for entry in read_dir {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!(
                    "error: read source directory {}: {error}",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || name == "target" {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                errors.push(format!("error: inspect source {}: {error}", path.display()));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            let target = match fs::canonicalize(&path) {
                Ok(target) => target,
                Err(error) => {
                    errors.push(format!(
                        "error: source link {} is unreadable: {error}",
                        path.display()
                    ));
                    continue;
                }
            };
            if !target.starts_with(root) {
                errors.push(format!(
                    "error: source link escapes package root: {}",
                    path.display()
                ));
            }
            continue;
        }
        if metadata.is_dir() {
            collect_aura_paths(&path, root, out, errors);
        } else if metadata.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("aura")
        {
            out.push(path);
        }
    }
}

fn parse_source(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| format!("error: source {} is not UTF-8: {error}", path.display()))?;
    parse_file(source)
        .map(|_| ())
        .map_err(|error| format!("error: parse source {}: {}", path.display(), error.message))
}

fn validate_dependencies(root: &Path, toml: &AuraToml, errors: &mut Vec<String>) {
    let lock = match read_lock(root) {
        Ok(lock) => lock,
        Err(error) => {
            errors.push(error);
            None
        }
    };
    let mut dependencies = toml.dependencies.iter().collect::<Vec<_>>();
    dependencies.sort_by(|left, right| left.0.cmp(right.0));
    for (name, dep) in dependencies {
        if !valid_package_name(name) {
            errors.push(format!(
                "error: dependency name is unsafe or empty: `{name}`"
            ));
        }
        match dep {
            DepSpec::Path(path) => {
                if Path::new(path).is_absolute() || path.trim().is_empty() {
                    errors.push(format!(
                        "error: dependency `{name}` has an unsafe path `{path}`"
                    ));
                    continue;
                }
                let dependency_root = root.join(path);
                if !dependency_root.exists() {
                    errors.push(format!(
                        "error: dependency `{name}` path not found: {}",
                        dependency_root.display()
                    ));
                } else if dependency_root.is_dir() && !dependency_root.join("aura.toml").is_file() {
                    errors.push(format!(
                        "error: dependency `{name}` path has no aura.toml: {}",
                        dependency_root.display()
                    ));
                }
            }
            DepSpec::Version(requirement) => {
                let parsed = match parse_req(requirement) {
                    Ok(req) => req,
                    Err(error) => {
                        errors.push(format!(
                            "error: dependency `{name}` has invalid version requirement `{requirement}`: {error}"
                        ));
                        continue;
                    }
                };
                let Some(lock) = lock.as_ref() else {
                    errors.push(format!(
                        "error: registry dependency `{name}` is not locked; dry-run does not resolve or fetch dependencies"
                    ));
                    continue;
                };
                let Some(entry) = lock.packages.get(name) else {
                    errors.push(format!(
                        "error: aura.lock is missing registry dependency `{name}`"
                    ));
                    continue;
                };
                if !entry.is_registry() {
                    errors.push(format!(
                        "error: aura.lock dependency `{name}` is not a registry pin"
                    ));
                    continue;
                }
                let Some(version) = entry.version.as_deref() else {
                    errors.push(format!(
                        "error: aura.lock registry dependency `{name}` is missing version"
                    ));
                    continue;
                };
                match parse_version(version) {
                    Ok(version) if parsed.matches(&version) => {}
                    Ok(_) => errors.push(format!(
                        "error: locked version `{version}` for `{name}` does not satisfy `{requirement}`"
                    )),
                    Err(error) => errors.push(format!("error: locked version for `{name}` is invalid: {error}")),
                }
                let Some(checksum) = entry.checksum.as_deref() else {
                    errors.push(format!(
                        "error: aura.lock registry dependency `{name}` is missing checksum"
                    ));
                    continue;
                };
                if !valid_checksum(checksum) {
                    errors.push(format!(
                        "error: registry dependency `{name}` has invalid checksum `{checksum}`"
                    ));
                }
            }
        }
    }
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c.is_whitespace() || c == '/' || c == '\\')
}

fn validate_release_version(version: &str) -> Result<(), String> {
    let numeric = version
        .split_once('-')
        .map(|(value, _)| value)
        .unwrap_or(version);
    if version.starts_with('v') || numeric.split('.').count() != 3 {
        return Err(format!(
            "error: package version `{version}` must be major.minor.patch (optional prerelease)"
        ));
    }
    parse_version(version)
        .map(|_| ())
        .map_err(|error| format!("error: package version: {error}"))
}

fn valid_checksum(checksum: &str) -> bool {
    let checksum = checksum.trim();
    let checksum = checksum
        .strip_prefix("sha256:")
        .or_else(|| checksum.strip_prefix("SHA256:"))
        .unwrap_or(checksum);
    checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn format_errors(errors: &[String]) -> String {
    format!("publish dry-run rejected:\n{}", errors.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("aura-publish-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        root
    }

    fn write(path: &Path, contents: &str) {
        let mut file = fs::File::create(path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn dry_run_previews_deterministic_archive_without_writing() {
        let root = package_root("valid");
        write(
            &root.join("aura.toml"),
            "[package]\nname = \"demo.publish\"\nversion = \"1.2.3\"\n",
        );
        write(
            &root.join("src/main.aura"),
            "package demo.publish\nfun main() {}\n",
        );
        let before = fs::read(root.join("aura.toml")).unwrap();
        let preview = publish_dry_run(&root).expect("preview");
        assert_eq!(preview.archive_name, "demo.publish-1.2.3.crate");
        assert_eq!(preview.checksum, archive_sha256(&preview.archive));
        assert!(preview.signature.is_none());
        assert!(preview.render().contains("registry mutation none"));
        assert_eq!(before, fs::read(root.join("aura.toml")).unwrap());
        assert!(!root.join("aura.lock").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dry_run_reports_all_invalid_inputs_before_archive() {
        let root = package_root("invalid");
        write(&root.join("aura.toml"), "[package]\nname = \"bad name\"\nversion = \"1.2\"\n[dependencies]\nmissing = { path = \"../absent\" }\n");
        write(&root.join("src/main.aura"), "package other\nfun main( {\n");
        let error = publish_dry_run(&root).unwrap_err();
        assert!(error.contains("unsafe or empty"), "{error}");
        assert!(error.contains("major.minor.patch"), "{error}");
        assert!(error.contains("path not found"), "{error}");
        assert!(error.contains("parse source"), "{error}");
        assert!(!root.join("aura.lock").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_dependency_requires_valid_read_only_lock_pin() {
        let root = package_root("registry");
        write(&root.join("aura.toml"), "[package]\nname = \"demo.publish\"\nversion = \"1.2.3\"\n[dependencies]\ntiny = \"0.1\"\n");
        write(
            &root.join("src/main.aura"),
            "package demo.publish\nfun main() {}\n",
        );
        write(&root.join("aura.lock"), "tiny = { version = \"0.1.0\", checksum = \"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\", source = \"registry\" }\n");
        let preview = publish_dry_run(&root).expect("locked preview");
        assert_eq!(preview.dependency_count, 1);
        let _ = fs::remove_dir_all(root);
    }
}
