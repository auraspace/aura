//! Direct package-origin transport.
//!
//! The Git implementation is the authoritative v1 origin.  The resolution
//! result deliberately contains origin identity and immutable revision so a
//! future proxy can serve the same object without changing lock semantics.

use super::archive::{archive_sha256, build_source_archive};
use super::semver::{parse_req, parse_version, Version};
use super::toml::DepSpec;
use std::cmp::Ordering;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

const GIT_TOKEN_ENV: &str = "AURA_REGISTRY_TOKEN";
const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
static TEMP_CHECKOUT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
pub(crate) const ORIGIN_PROTOCOL_VERSION: &str = "aura-origin-v1";

/// Read-only proxy contract reserved for a later cache implementation.  A
/// proxy serves these objects, but never becomes the package identity.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProxyContract {
    base_url: String,
}

#[allow(dead_code)]
impl ProxyContract {
    pub(crate) fn new(base_url: &str) -> Result<Self, String> {
        let base_url = base_url.trim_end_matches('/');
        if !base_url.starts_with("https://") {
            return Err("error: registry proxy URL must use HTTPS".into());
        }
        Ok(Self {
            base_url: base_url.into(),
        })
    }

    pub(crate) fn object_url(&self, module: &str, object: &str) -> String {
        format!(
            "{}/{}/@v/{}",
            self.base_url,
            module.trim_matches('/'),
            object
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OriginResolution {
    pub(crate) source: String,
    pub(crate) version: String,
    pub(crate) rev: String,
    pub(crate) checksum: String,
    pub(crate) archive: Vec<u8>,
}

/// Resolve and materialize a direct Git origin from a manifest dependency.
pub(crate) fn resolve_git(name: &str, dep: &DepSpec) -> Result<OriginResolution, String> {
    let DepSpec::Git {
        source,
        subdir,
        version,
        tag,
        rev,
    } = dep
    else {
        return Err(format!("error: package `{name}` is not a Git dependency"));
    };
    let source = if let Some(subdir) = subdir {
        format!("{source}#subdir={subdir}")
    } else {
        source.clone()
    };
    let source = canonical_source(&source)?;
    let selected = match (tag.as_deref(), rev.as_deref()) {
        (Some(tag), None) => {
            let version = version_from_tag(tag)?;
            let rev = resolve_ref(&source, tag)?;
            (version, rev)
        }
        (None, Some(rev)) => {
            let resolved = resolve_ref(&source, rev)?;
            let version = version.as_deref().unwrap_or("0.0.0");
            let version = version.strip_prefix('v').unwrap_or(version);
            let version = parse_version(version)
                .map_err(|e| format!("error: invalid Git dependency version: {e}"))?;
            (version, resolved)
        }
        (None, None) => select_version(&source, version.as_deref())?,
        (Some(_), Some(_)) => {
            return Err(format!(
                "error: package `{name}` Git dependency cannot set both `tag` and `rev`"
            ));
        }
    };
    let version_hint = selected.0.to_string_canonical();
    let (version, archive) = archive_revision(name, &source, &selected.1, &version_hint)?;
    let checksum = archive_sha256(&archive);
    Ok(OriginResolution {
        source,
        version,
        rev: selected.1,
        checksum,
        archive,
    })
}

pub(crate) fn canonical_source(source: &str) -> Result<String, String> {
    let source = source.trim();
    let (base, fragment) = source.split_once('#').unwrap_or((source, ""));
    let base = base.trim().trim_end_matches('/').trim_end_matches(".git");
    if base.is_empty() {
        return Err("error: Git origin source is empty".into());
    }
    if base.contains('@') && (base.starts_with("http://") || base.starts_with("https://")) {
        return Err("error: Git origin URLs must not contain embedded credentials".into());
    }
    if !(base.starts_with("https://")
        || (cfg!(test) && base.starts_with("http://"))
        || base.starts_with("ssh://")
        || base.starts_with("git@")
        || base.starts_with("file://")
        || Path::new(base).is_absolute())
    {
        return Err(format!(
            "error: unsupported Git origin `{base}`; use HTTPS, SSH, file://, or an absolute local path"
        ));
    }
    if fragment.is_empty() {
        return Ok(base.to_string());
    }
    let subdir = fragment
        .strip_prefix("subdir=")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "error: unsupported Git origin fragment; use `#subdir=path`".to_string())?;
    let path = Path::new(subdir);
    if path.is_absolute()
        || subdir
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("error: Git origin subdir must be a normalized relative path".into());
    }
    Ok(format!("{base}#subdir={subdir}"))
}

fn git_repository(source: &str) -> &str {
    source
        .split_once("#subdir=")
        .map(|(base, _)| base)
        .unwrap_or(source)
}

fn package_subdir(source: &str) -> Option<&str> {
    source.split_once("#subdir=").map(|(_, subdir)| subdir)
}

fn select_version(source: &str, requirement: Option<&str>) -> Result<(Version, String), String> {
    let requirement = requirement
        .map(parse_req)
        .transpose()
        .map_err(|e| format!("error: invalid Git version requirement: {e}"))?;
    let mut candidates = list_tags(source)?
        .into_iter()
        .filter_map(|(tag, rev)| {
            let version = version_from_tag(&tag).ok()?;
            if requirement
                .as_ref()
                .is_some_and(|req| !req.matches(&version))
            {
                return None;
            }
            Some((version, rev))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| match a.0.cmp(&b.0) {
        Ordering::Equal => a.1.cmp(&b.1),
        other => other,
    });
    candidates.pop().ok_or_else(|| {
        format!(
            "error: no compatible semver tag found at Git origin `{source}`{}",
            requirement
                .as_ref()
                .map(|req| format!(" for `{}`", req.raw))
                .unwrap_or_default()
        )
    })
}

fn list_tags(source: &str) -> Result<Vec<(String, String)>, String> {
    let repository = git_repository(source);
    let output = run_git(source, &["ls-remote", "--tags", repository])?;
    let mut tags = std::collections::BTreeMap::<String, String>::new();
    for line in output.lines() {
        let Some((rev, reference)) = line.split_once('\t') else {
            continue;
        };
        let Some(tag) = reference
            .strip_prefix("refs/tags/")
            .and_then(|tag| tag.strip_suffix("^{}"))
            .or_else(|| reference.strip_prefix("refs/tags/"))
        else {
            continue;
        };
        if is_commit_sha(rev) {
            if reference.ends_with("^{}") {
                tags.insert(tag.to_string(), rev.to_string());
            } else {
                tags.entry(tag.to_string())
                    .or_insert_with(|| rev.to_string());
            }
        }
    }
    Ok(tags.into_iter().collect())
}

fn resolve_ref(source: &str, reference: &str) -> Result<String, String> {
    if reference.is_empty()
        || reference.starts_with('-')
        || reference.chars().any(char::is_whitespace)
    {
        return Err(format!("error: invalid Git reference `{reference}`"));
    }
    let repository = git_repository(source);
    let output = run_git(
        source,
        &[
            "ls-remote",
            repository,
            reference,
            &format!("{reference}^{{}}"),
        ],
    )?;
    let mut peeled = None;
    let mut plain = None;
    for line in output.lines() {
        let Some((rev, ref_name)) = line.split_once('\t') else {
            continue;
        };
        if !is_commit_sha(rev) {
            continue;
        }
        if ref_name.ends_with("^{}") {
            peeled = Some(rev.to_string());
        } else {
            plain = Some(rev.to_string());
        }
    }
    peeled.or(plain).ok_or_else(|| {
        format!("error: Git reference `{reference}` was not found at origin `{source}`")
    })
}

fn version_from_tag(tag: &str) -> Result<Version, String> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    parse_version(version).map_err(|e| format!("error: invalid semver Git tag `{tag}`: {e}"))
}

fn archive_revision(
    name: &str,
    source: &str,
    rev: &str,
    version_hint: &str,
) -> Result<(String, Vec<u8>), String> {
    let repository = git_repository(source);
    let root = temp_checkout_root(name, rev);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|e| format!("error: create Git staging directory: {e}"))?;
    let result = (|| {
        run_git(
            source,
            &[
                "clone",
                "--quiet",
                "--no-checkout",
                repository,
                path_arg(&root),
            ],
        )?;
        if !root.join(".git").exists() {
            return Err(format!(
                "Git clone completed without a repository at `{}`",
                root.display()
            ));
        }
        run_git_in(&root, &["checkout", "--quiet", "--detach", rev])?;
        let package_root = package_subdir(source)
            .map(|subdir| root.join(subdir))
            .unwrap_or_else(|| root.clone());
        if !package_root.is_dir() {
            return Err(format!(
                "Git origin package subdir `{}` does not exist",
                package_root.display()
            ));
        }
        let entries = collect_source_entries(&package_root)?;
        let manifest = entries
            .iter()
            .find(|(path, _)| path == "aura.toml")
            .ok_or_else(|| "Git origin has no aura.toml at the selected revision".to_string())?;
        let manifest_text = String::from_utf8(manifest.1.clone())
            .map_err(|e| format!("Git origin aura.toml is not UTF-8: {e}"))?;
        let manifest = super::toml::parse_aura_toml(&manifest_text)
            .map_err(|e| format!("Git origin aura.toml is invalid: {e}"))?;
        let version = match manifest.package_version.as_deref() {
            Some(declared) => {
                let declared = parse_version(declared)
                    .map_err(|e| format!("Git origin package version is invalid: {e}"))?;
                let hint = parse_version(version_hint)
                    .map_err(|e| format!("Git origin version selector is invalid: {e}"))?;
                if version_hint != "0.0.0" && hint != declared {
                    return Err(format!(
                        "Git tag/revision version `{version_hint}` does not match package manifest version `{}`",
                        declared.to_string_canonical()
                    ));
                }
                declared.to_string_canonical()
            }
            None if version_hint != "0.0.0" => version_hint.to_string(),
            None => {
                return Err(
                    "Git origin aura.toml is missing [package].version for an untagged revision"
                        .into(),
                )
            }
        };
        let archive = build_source_archive(name, &version, &entries)?;
        Ok((version, archive))
    })();
    let _ = fs::remove_dir_all(&root);
    result.map_err(|error| format!("error: fetch Git origin `{source}` at `{rev}`: {error}"))
}

fn collect_source_entries(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut paths = Vec::new();
    collect_files(root, &mut paths)?;
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| format!("error: Git source path: {e}"))?;
            let name = relative.to_string_lossy().replace('\\', "/");
            let bytes =
                fs::read(&path).map_err(|e| format!("error: read Git source `{name}`: {e}"))?;
            Ok((name, bytes))
        })
        .collect()
}

fn collect_files(current: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut children = fs::read_dir(current)
        .map_err(|e| {
            format!(
                "error: read Git source directory {}: {e}",
                current.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    children.sort();
    for child in children {
        if child.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        if child.is_dir() {
            collect_files(&child, out)?;
        } else if child.is_file() {
            out.push(child);
        }
    }
    Ok(())
}

fn temp_checkout_root(name: &str, rev: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = TEMP_CHECKOUT_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    env::temp_dir().join(format!(
        "aura-origin-{}-{}-{}-{nonce}-{sequence}",
        name.replace(|c: char| !c.is_ascii_alphanumeric(), "-"),
        &rev[..rev.len().min(12)],
        std::process::id()
    ))
}

fn is_commit_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn path_arg(path: &Path) -> &str {
    path.to_str().expect("temporary path must be UTF-8")
}

fn run_git(source: &str, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.args(args);
    configure_git_credentials(&mut command);
    run_command(command, source)
}

fn run_git_in(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    configure_git_credentials(&mut command);
    run_command(command, root.to_string_lossy().as_ref())
}

fn configure_git_credentials(command: &mut Command) {
    let token = env::var(GIT_TOKEN_ENV)
        .ok()
        .or_else(|| env::var(GITHUB_TOKEN_ENV).ok());
    if let Some(token) = token {
        // Git receives the header through environment-backed config, so the
        // credential never enters argv, lockfiles, or diagnostic strings.
        command.env("GIT_CONFIG_COUNT", "1");
        command.env("GIT_CONFIG_KEY_0", "http.extraheader");
        command.env(
            "GIT_CONFIG_VALUE_0",
            format!("Authorization: Bearer {token}"),
        );
    }
}

fn run_command(mut command: Command, context: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|e| format!("could not execute Git for `{context}`: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(redact_git_error(stderr.trim(), context));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| format!("Git returned non-UTF-8 output for `{context}`: {e}"))
}

fn redact_git_error(error: &str, context: &str) -> String {
    let mut safe = error.replace(context, "<origin>");
    if let Ok(token) = env::var(GIT_TOKEN_ENV) {
        safe = safe.replace(&token, "<redacted>");
    }
    if let Ok(token) = env::var(GITHUB_TOKEN_ENV) {
        safe = safe.replace(&token, "<redacted>");
    }
    if safe.is_empty() {
        "Git command failed without diagnostics".into()
    } else {
        safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Aura Test")
            .env("GIT_AUTHOR_EMAIL", "aura@example.invalid")
            .env("GIT_COMMITTER_NAME", "Aura Test")
            .env("GIT_COMMITTER_EMAIL", "aura@example.invalid")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn fixture() -> (PathBuf, String) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_CHECKOUT_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
        let root = env::temp_dir().join(format!(
            "aura-origin-test-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("aura.toml"),
            "[package]\nname = \"tiny\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        fs::write(root.join("src/main.aura"), "fun main() {}\n").unwrap();
        git(&root, &["init", "--quiet"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "initial"]);
        git(&root, &["tag", "v1.2.3"]);
        (root.clone(), root.display().to_string())
    }

    #[test]
    fn resolves_tag_to_commit_and_archives_source() {
        let (_root, source) = fixture();
        let dep = DepSpec::Git {
            source,
            subdir: None,
            version: None,
            tag: Some("v1.2.3".into()),
            rev: None,
        };
        let resolved = resolve_git("tiny", &dep).unwrap();
        assert_eq!(resolved.version, "1.2.3");
        assert!(is_commit_sha(&resolved.rev));
        assert_eq!(resolved.checksum.len(), 64);
        assert!(!resolved.archive.is_empty());
    }

    #[test]
    fn selects_highest_compatible_tag() {
        let (root, source) = fixture();
        fs::write(
            root.join("aura.toml"),
            "[package]\nname = \"tiny\"\nversion = \"1.3.0\"\n",
        )
        .unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "release-1.3"]);
        git(&root, &["tag", "v1.3.0"]);
        let dep = DepSpec::Git {
            source,
            subdir: None,
            version: Some("1.2".into()),
            tag: None,
            rev: None,
        };
        let resolved = resolve_git("tiny", &dep).unwrap();
        assert_eq!(resolved.version, "1.3.0");
    }

    #[test]
    fn rejects_embedded_http_credentials() {
        let error = canonical_source("https://user:secret@example.com/pkg").unwrap_err();
        assert!(error.contains("embedded credentials"));
        assert!(!error.contains("secret"));
    }

    #[test]
    fn direct_revision_uses_manifest_version() {
        let (_root, source) = fixture();
        let dep = DepSpec::Git {
            source,
            subdir: None,
            version: None,
            tag: None,
            rev: Some("HEAD".into()),
        };
        let resolved = resolve_git("tiny", &dep).unwrap();
        assert_eq!(resolved.version, "1.2.3");
        assert!(is_commit_sha(&resolved.rev));
    }

    #[test]
    fn resolves_versionless_monorepo_subdir_from_tag() {
        let (root, source) = fixture();
        fs::create_dir_all(root.join("lib/src")).unwrap();
        fs::write(
            root.join("lib/aura.toml"),
            "[package]\nname = \"tiny.lib\"\n",
        )
        .unwrap();
        fs::write(root.join("lib/src/main.aura"), "fun main() {}\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "lib"]);
        git(&root, &["tag", "v1.2.4"]);
        let dep = DepSpec::Git {
            source,
            subdir: Some("lib".into()),
            version: None,
            tag: Some("v1.2.4".into()),
            rev: None,
        };
        let resolved = resolve_git("tiny.lib", &dep).unwrap();
        assert_eq!(resolved.version, "1.2.4");
        assert!(!resolved.archive.is_empty());
    }

    #[test]
    fn proxy_contract_preserves_origin_read_shapes() {
        let proxy = ProxyContract::new("https://proxy.example").unwrap();
        assert_eq!(
            proxy.object_url("example/pkg", "list"),
            "https://proxy.example/example/pkg/@v/list"
        );
        assert!(ProxyContract::new("http://proxy.example").is_err());
    }
}
