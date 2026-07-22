//! Offline registry index client (C13i).
//!
//! Reads package metadata from a local index tree (fixture or cache):
//!
//! ```text
//! <index>/
//!   config.json                 # optional download template
//!   packages/<name>/versions.json
//!   packages/<aa>/<bb>/<name>/versions.json   # sparse layout (RFC-005)
//! ```
//!
//! Prefer `AURA_REGISTRY_INDEX` for tests; otherwise `~/.aura/registry/index`.
//! HTTPS metadata is fetched on demand by [`RegistryIndex::open_url`].

use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::fetch::read_crate_bytes;
use super::fetch::{read_crate_bytes_bounded, verify_sha256, MAX_ARTIFACT_BYTES};
use super::semver::parse_version;

/// Env override for the index root (fixture or cache). Preferred in tests.
pub const ENV_REGISTRY_INDEX: &str = "AURA_REGISTRY_INDEX";

/// Optional index `config.json` fields (RFC-005 §6.6.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryConfig {
    /// Download URL template, e.g. GitHub Release asset pattern.
    pub dl: Option<String>,
    /// Index / API base URL (informational for offline MVP).
    pub api: Option<String>,
    pub github_api: Option<String>,
}

/// One published version record from `versions.json` (RFC-005 fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionMeta {
    pub name: String,
    /// Semver string (`vers` in the index document).
    pub vers: String,
    /// sha256 of the `.crate` tarball (`cksum`), may include `sha256:` prefix.
    pub cksum: String,
    pub yanked: bool,
    /// `owner/repo` used to fill the download template.
    pub repository: Option<String>,
    /// Canonical targets which may consume this release (for example
    /// `linux-amd64` or `darwin-arm64`).  `None` means legacy metadata and is
    /// not eligible for U6 update discovery.
    pub targets: Option<Vec<String>>,
    /// Optional Aura toolchain compatibility bounds, inclusive.
    pub min_aura: Option<String>,
    pub max_aura: Option<String>,
    /// A revoked release must never be selected, even when it is newer.
    pub revoked: bool,
    pub revoke_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub meta: VersionMeta,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateDecision {
    Update(UpdateCandidate),
    NoUpdate { current: String },
    Unsupported { current: String, target: String },
    Revoked { version: String, reason: String },
}

impl UpdateDecision {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Update(_) => "update_available",
            Self::NoUpdate { .. } => "no_update",
            Self::Unsupported { .. } => "unsupported",
            Self::Revoked { .. } => "revoked",
        }
    }

    pub fn render_json(&self) -> String {
        match self {
            Self::Update(candidate) => format!(
                "{{\"ok\":true,\"code\":\"update_available\",\"version\":{},\"target\":{},\"checksum\":{},\"reason\":{}}}",
                json_string(&candidate.meta.vers),
                json_string(&candidate.target),
                json_string(&candidate.meta.cksum),
                json_string(&candidate.reason),
            ),
            Self::NoUpdate { current } => format!(
                "{{\"ok\":true,\"code\":\"no_update\",\"current\":{}}}",
                json_string(current)
            ),
            Self::Unsupported { current, target } => format!(
                "{{\"ok\":false,\"code\":\"unsupported\",\"current\":{},\"target\":{}}}",
                json_string(current),
                json_string(target),
            ),
            Self::Revoked { version, reason } => format!(
                "{{\"ok\":false,\"code\":\"revoked\",\"version\":{},\"reason\":{}}}",
                json_string(version),
                json_string(reason),
            ),
        }
    }
}

fn validate_update_metadata(meta: &VersionMeta) -> Result<(), String> {
    let checksum = super::fetch::normalize_cksum(&meta.cksum);
    if checksum.len() != 64 || !checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("error: registry metadata for `{}-{}` has invalid sha256 checksum", meta.name, meta.vers));
    }
    if meta.targets.as_ref().is_none_or(Vec::is_empty) {
        return Err(format!("error: registry metadata for `{}-{}` has no target list", meta.name, meta.vers));
    }
    if let Some(min) = &meta.min_aura {
        parse_version(min).map_err(|e| format!("error: invalid min_aura `{min}`: {e}"))?;
    }
    if let Some(max) = &meta.max_aura {
        parse_version(max).map_err(|e| format!("error: invalid max_aura `{max}`: {e}"))?;
    }
    Ok(())
}

fn target_matches(targets: Option<&[String]>, target: &str) -> bool {
    targets.is_some_and(|items| items.iter().any(|item| {
        item.eq_ignore_ascii_case(target)
            || (item.eq_ignore_ascii_case("linux-x86_64") && target == "linux-amd64")
            || (item.eq_ignore_ascii_case("darwin-x86_64") && target == "darwin-amd64")
    }))
}

fn toolchain_matches(meta: &VersionMeta, toolchain: &super::semver::Version) -> Result<bool, String> {
    if let Some(min) = &meta.min_aura {
        if *toolchain < parse_version(min).map_err(|e| format!("error: invalid min_aura `{min}`: {e}"))? {
            return Ok(false);
        }
    }
    if let Some(max) = &meta.max_aura {
        if *toolchain > parse_version(max).map_err(|e| format!("error: invalid max_aura `{max}`: {e}"))? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn current_target() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// Local registry index root (fixture directory or cache).
#[derive(Debug, Clone)]
pub struct RegistryIndex {
    root: PathBuf,
    config: RegistryConfig,
    remote: Option<String>,
}

impl RegistryIndex {
    /// Open an index at `root`. Errors if the directory does not exist.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(format!(
                "error: registry index not found: {}",
                root.display()
            ));
        }
        let config = load_config(&root.join("config.json"))?;
        Ok(Self {
            root,
            config,
            remote: None,
        })
    }

    /// Open an HTTPS registry index without writing metadata to disk.
    #[allow(dead_code)]
    pub fn open_url(base_url: &str) -> Result<Self, String> {
        if !(base_url.starts_with("https://") || cfg!(test) && base_url.starts_with("http://")) {
            return Err(format!(
                "error: registry index URL must use HTTPS: {base_url}"
            ));
        }
        let base = base_url.trim_end_matches('/').to_string();
        let config_bytes = read_crate_bytes(&format!("{base}/config.json"))?;
        let config_text = String::from_utf8(config_bytes)
            .map_err(|e| format!("error: registry config is not UTF-8: {e}"))?;
        let config =
            parse_config_json(&config_text).map_err(|e| format!("error: registry config: {e}"))?;
        Ok(Self {
            root: PathBuf::new(),
            config,
            remote: Some(base),
        })
    }

    /// Index root from `AURA_REGISTRY_INDEX`, else default cache path.
    pub fn from_env_or_default() -> Result<Self, String> {
        Self::open(index_root_from_env())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    /// All version strings for `name` (including yanked), in index order.
    #[cfg(test)]
    pub fn list_versions(&self, name: &str) -> Result<Vec<String>, String> {
        Ok(self
            .package_versions(name)?
            .into_iter()
            .map(|v| v.vers)
            .collect())
    }

    /// Non-yanked version strings only (for future semver resolve).
    #[cfg(test)]
    pub fn list_versions_unyanked(&self, name: &str) -> Result<Vec<String>, String> {
        Ok(self
            .package_versions(name)?
            .into_iter()
            .filter(|v| !v.yanked)
            .map(|v| v.vers)
            .collect())
    }

    /// Full metadata for every version of `name`.
    pub fn package_versions(&self, name: &str) -> Result<Vec<VersionMeta>, String> {
        let text = if let Some(base) = &self.remote {
            let rel = package_versions_rel_paths(name)
                .into_iter()
                .next()
                .expect("registry package path");
            let url = format!("{base}/{}", rel.to_string_lossy());
            let bytes = read_crate_bytes(&url)?;
            String::from_utf8(bytes)
                .map_err(|e| format!("error: registry metadata is not UTF-8: {e}"))?
        } else {
            let path = self
                .versions_path(name)
                .ok_or_else(|| format!("error: package `{name}` not found in registry index"))?;
            fs::read_to_string(&path).map_err(|e| format!("error: read {}: {e}", path.display()))?
        };
        parse_versions_json(&text)
            .map_err(|e| format!("error: registry metadata for `{name}`: {e}"))
    }

    /// Select the highest newer release whose metadata is valid, target
    /// compatible, and compatible with the running Aura toolchain.  This is a
    /// metadata-only operation: it does not resolve a download URL or fetch a
    /// payload, leaving activation explicitly to U7.
    pub fn discover_update(
        &self,
        name: &str,
        current_version: &str,
        aura_version: &str,
        target: &str,
    ) -> Result<UpdateDecision, String> {
        let current = parse_version(current_version)
            .map_err(|e| format!("error: current version `{current_version}`: {e}"))?;
        let toolchain = parse_version(aura_version)
            .map_err(|e| format!("error: Aura version `{aura_version}`: {e}"))?;
        let mut versions = self.package_versions(name)?;
        versions.sort_by(|a, b| {
            parse_version(&b.vers)
                .ok()
                .cmp(&parse_version(&a.vers).ok())
        });
        let mut saw_unsupported = false;
        let mut revoked = None;
        for meta in versions {
            if meta.name != name {
                return Err(format!("error: registry metadata name mismatch: expected `{name}`, got `{}`", meta.name));
            }
            let version = parse_version(&meta.vers)
                .map_err(|e| format!("error: registry metadata version `{}`: {e}", meta.vers))?;
            if version <= current || meta.yanked {
                continue;
            }
            validate_update_metadata(&meta)?;
            if meta.revoked {
                revoked = Some((meta.vers.clone(), meta.revoke_reason.clone().unwrap_or_else(|| "registry revoked this release".into())));
                continue;
            }
            if !target_matches(meta.targets.as_deref(), target) || !toolchain_matches(&meta, &toolchain)? {
                saw_unsupported = true;
                continue;
            }
            return Ok(UpdateDecision::Update(UpdateCandidate {
                reason: format!("{} is newer and compatible with {target}", meta.vers),
                meta,
                target: target.into(),
            }));
        }
        if let Some((version, reason)) = revoked {
            return Ok(UpdateDecision::Revoked { version, reason });
        }
        if saw_unsupported {
            return Ok(UpdateDecision::Unsupported { current: current_version.into(), target: target.into() });
        }
        Ok(UpdateDecision::NoUpdate { current: current_version.into() })
    }

    /// Resolve the payload source for a previously validated U6 candidate.
    /// Keeping this separate from discovery makes it impossible for metadata
    /// inspection to perform a download accidentally.
    pub fn update_source(&self, candidate: &UpdateCandidate) -> Result<String, String> {
        super::fetch::crate_source_for_meta(
            &self.root,
            self.config.dl.as_deref(),
            &candidate.meta,
        )
    }

    /// Metadata for an exact version pin.
    #[cfg(test)]
    pub fn get_version_meta(&self, name: &str, version: &str) -> Result<VersionMeta, String> {
        let versions = self.package_versions(name)?;
        versions
            .into_iter()
            .find(|v| v.vers == version)
            .ok_or_else(|| {
                format!("error: package `{name}` version `{version}` not found in registry index")
            })
    }

    /// Resolve `versions.json` path: flat fixture first, then sparse (RFC-005).
    fn versions_path(&self, name: &str) -> Option<PathBuf> {
        for rel in package_versions_rel_paths(name) {
            let p = self.root.join(&rel);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }
}

/// U7's deliberately bounded activation contract. The payload is an Aura
/// executable, not a source archive; its checksum is verified before the
/// active path is touched. Signature verification is reported as deferred
/// because the alpha metadata has no real signing primitive or key policy.
pub const UPDATE_SIGNATURE_STATUS: &str = "deferred";
const UPDATE_STATE_SUFFIX: &str = ".aura-update-state";
const UPDATE_TEMP_SUFFIX: &str = ".aura-update-staging";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateActivation {
    pub version: String,
    pub checksum: String,
    pub signature: &'static str,
    pub executable: PathBuf,
    pub rollback: PathBuf,
    pub state: PathBuf,
}

impl UpdateActivation {
    pub fn render_json(&self) -> String {
        format!(
            "{{\"ok\":true,\"code\":\"activated\",\"version\":{},\"checksum\":{},\"signature\":{},\"executable\":{},\"rollback\":{}}}",
            json_string(&self.version),
            json_string(&self.checksum),
            json_string(self.signature),
            json_string(&self.executable.display().to_string()),
            json_string(&self.rollback.display().to_string()),
        )
    }
}

/// Download, verify, stage, and atomically activate a candidate executable.
/// Every fallible operation before the final rename leaves `active` untouched;
/// if the rename fails, the staged file and newly-created rollback copy are
/// removed while the old executable remains active.
pub fn activate_update(
    candidate: &UpdateCandidate,
    source: &str,
    active: impl AsRef<Path>,
) -> Result<UpdateActivation, String> {
    let active = active.as_ref();
    let active_meta = fs::symlink_metadata(active)
        .map_err(|error| format!("error: cannot inspect active executable {}: {error}", active.display()))?;
    if !active_meta.file_type().is_file() {
        return Err(format!("error: active executable is not a regular file: {}", active.display()));
    }
    let parent = active.parent().unwrap_or_else(|| Path::new("."));
    let file_name = active
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("error: active executable has no valid file name: {}", active.display()))?;
    let nonce = update_nonce();
    let staging = parent.join(format!(".{file_name}{UPDATE_TEMP_SUFFIX}-{nonce}"));
    let rollback = parent.join(format!(".{file_name}.aura-rollback-{nonce}"));
    let state = parent.join(format!(".{file_name}{UPDATE_STATE_SUFFIX}"));

    let bytes = match read_crate_bytes_bounded(source, MAX_ARTIFACT_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => return Err(format!("error: update download failed: {error}")),
    };
    verify_sha256(&bytes, &candidate.meta.cksum)
        .map_err(|error| format!("error: update verification failed: {error}"))?;

    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|error| format!("error: create update staging file {}: {error}", staging.display()))?;
    if let Err(error) = staged.write_all(&bytes).and_then(|_| staged.sync_all()) {
        let _ = fs::remove_file(&staging);
        return Err(format!("error: write update staging file {}: {error}", staging.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = fs::set_permissions(&staging, fs::Permissions::from_mode(active_meta.permissions().mode())) {
            let _ = fs::remove_file(&staging);
            return Err(format!("error: preserve executable permissions: {error}"));
        }
    }
    drop(staged);

    if let Err(error) = fs::copy(active, &rollback) {
        let _ = fs::remove_file(&staging);
        return Err(format!("error: retain rollback executable {}: {error}", rollback.display()));
    }
    if let Err(error) = sync_file(&rollback) {
        let _ = fs::remove_file(&staging);
        let _ = fs::remove_file(&rollback);
        return Err(format!("error: sync rollback executable {}: {error}", rollback.display()));
    }

    let state_contents = format!(
        "version={}\nchecksum={}\nactive={}\nrollback={}\nsignature={}\n",
        candidate.meta.vers,
        super::fetch::normalize_cksum(&candidate.meta.cksum),
        active.display(),
        rollback.display(),
        UPDATE_SIGNATURE_STATUS,
    );
    if let Err(error) = write_atomic_state(&state, &state_contents, &nonce) {
        let _ = fs::remove_file(&staging);
        let _ = fs::remove_file(&rollback);
        return Err(error);
    }

    if let Err(error) = fs::rename(&staging, active) {
        let _ = fs::remove_file(&staging);
        let _ = fs::remove_file(&rollback);
        let _ = fs::remove_file(&state);
        return Err(format!("error: atomic update activation failed: {error}"));
    }
    Ok(UpdateActivation {
        version: candidate.meta.vers.clone(),
        checksum: super::fetch::normalize_cksum(&candidate.meta.cksum),
        signature: UPDATE_SIGNATURE_STATUS,
        executable: active.to_path_buf(),
        rollback,
        state,
    })
}

fn update_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
        ^ u128::from(std::process::id())
}

fn sync_file(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("error: sync {}: {error}", path.display()))
}

fn write_atomic_state(path: &Path, contents: &str, nonce: &u128) -> Result<(), String> {
    let tmp = path.with_file_name(format!("{}.tmp-{nonce}", path.file_name().unwrap().to_string_lossy()));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)
            .map_err(|error| format!("error: create update state {}: {error}", tmp.display()))?;
        file.write_all(contents.as_bytes()).and_then(|_| file.sync_all())
            .map_err(|error| format!("error: write update state {}: {error}", tmp.display()))?;
        fs::rename(&tmp, path)
            .map_err(|error| format!("error: publish update state {}: {error}", path.display()))
    })();
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result
}

/// Minimal, explicit upload contract used by U5.
///
/// The endpoint is intentionally not inferred from a registry's download/index
/// API: it is a small Aura-specific fixture contract until a production API is
/// standardized. A successful response must be HTTP 201 with a JSON object
/// containing `status`, `name`, `version`, and `checksum`.
pub const PUBLISH_PATH: &str = "/api/v1/publish";
const PUBLISH_ATTEMPTS: usize = 3;
const MAX_PUBLISH_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishErrorKind {
    Auth,
    Conflict,
    Rejected,
    RetryExhausted,
    Indeterminate,
    Protocol,
}

impl PublishErrorKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Conflict => "version_conflict",
            Self::Rejected => "rejected",
            Self::RetryExhausted => "retry_exhausted",
            Self::Indeterminate => "indeterminate",
            Self::Protocol => "protocol",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishError {
    pub kind: PublishErrorKind,
    pub status: Option<u16>,
    pub attempts: usize,
    pub message: String,
}

impl PublishError {
    pub fn render_json(&self) -> String {
        format!(
            "{{\"ok\":false,\"code\":\"{}\",\"status\":{},\"attempts\":{},\"message\":{}}}",
            self.kind.code(),
            self.status
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".into()),
            self.attempts,
            json_string(&self.message),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    pub name: String,
    pub version: String,
    pub checksum: String,
    pub attempts: usize,
}

impl PublishReceipt {
    pub fn render_json(&self) -> String {
        format!(
            "{{\"ok\":true,\"status\":201,\"name\":{},\"version\":{},\"checksum\":{},\"attempts\":{}}}",
            json_string(&self.name),
            json_string(&self.version),
            json_string(&self.checksum),
            self.attempts,
        )
    }
}

/// Upload a validated U4 preview to the frozen minimal endpoint.
pub fn publish_upload(
    base_url: &str,
    token: Option<&str>,
    preview: &super::publish::PublishPreview,
) -> Result<PublishReceipt, PublishError> {
    if !(base_url.starts_with("https://") || cfg!(test) && base_url.starts_with("http://")) {
        return Err(PublishError {
            kind: PublishErrorKind::Rejected,
            status: None,
            attempts: 0,
            message: "registry URL must use HTTPS".into(),
        });
    }
    if preview.archive.len() > 64 * 1024 * 1024 {
        return Err(PublishError {
            kind: PublishErrorKind::Rejected,
            status: Some(413),
            attempts: 0,
            message: "archive exceeds the 64 MiB upload limit".into(),
        });
    }
    let url = format!("{}{}", base_url.trim_end_matches('/'), PUBLISH_PATH);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(30))
        .timeout_write(std::time::Duration::from_secs(30))
        .build();
    let mut last = None;
    for attempt in 1..=PUBLISH_ATTEMPTS {
        let mut request = agent
            .post(&url)
            .set("Content-Type", "application/gzip")
            .set("X-Aura-Package", &preview.package)
            .set("X-Aura-Version", &preview.version)
            .set("X-Aura-Sha256", &preview.checksum);
        if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        match request.send_bytes(&preview.archive) {
            Ok(response) => {
                let status = response.status();
                let mut reader = response.into_reader();
                let body = match read_bounded(&mut reader, MAX_PUBLISH_RESPONSE_BYTES) {
                    Ok(body) => body,
                    Err(message) => {
                        return Err(PublishError {
                            kind: PublishErrorKind::Protocol,
                            status: Some(status),
                            attempts: attempt,
                            message,
                        });
                    }
                };
                if status == 201 {
                    return parse_receipt(&body, preview, attempt);
                }
                let kind = match status {
                    401 | 403 => PublishErrorKind::Auth,
                    409 => PublishErrorKind::Conflict,
                    400 | 413 => PublishErrorKind::Rejected,
                    500..=599 if attempt < PUBLISH_ATTEMPTS => {
                        last = Some(format!("registry returned HTTP {status}"));
                        continue;
                    }
                    500..=599 => PublishErrorKind::RetryExhausted,
                    _ => PublishErrorKind::Protocol,
                };
                return Err(PublishError {
                    kind,
                    status: Some(status),
                    attempts: attempt,
                    message: format!("registry returned HTTP {status}"),
                });
            }
            Err(ureq::Error::Status(status, _)) if status >= 500 && attempt < PUBLISH_ATTEMPTS => {
                last = Some(format!("registry returned HTTP {status}"));
            }
            Err(ureq::Error::Status(status, _)) => {
                let kind = match status {
                    401 | 403 => PublishErrorKind::Auth,
                    409 => PublishErrorKind::Conflict,
                    400 | 413 => PublishErrorKind::Rejected,
                    500..=599 => PublishErrorKind::RetryExhausted,
                    _ => PublishErrorKind::Protocol,
                };
                return Err(PublishError {
                    kind,
                    status: Some(status),
                    attempts: attempt,
                    message: format!("registry returned HTTP {status}"),
                });
            }
            Err(ureq::Error::Transport(error)) => {
                // A POST may have reached and committed at the registry. Never
                // turn an exhausted transport failure into a false success.
                if attempt == PUBLISH_ATTEMPTS {
                    return Err(PublishError {
                        kind: PublishErrorKind::Indeterminate,
                        status: None,
                        attempts: attempt,
                        message: format!("publish transport outcome is unknown: {error}"),
                    });
                }
                last = Some(error.to_string());
            }
        }
    }
    Err(PublishError {
        kind: PublishErrorKind::RetryExhausted,
        status: None,
        attempts: PUBLISH_ATTEMPTS,
        message: last.unwrap_or_else(|| "registry publish retries exhausted".into()),
    })
}

fn parse_receipt(
    body: &[u8],
    preview: &super::publish::PublishPreview,
    attempts: usize,
) -> Result<PublishReceipt, PublishError> {
    let text = std::str::from_utf8(body).map_err(|error| PublishError {
        kind: PublishErrorKind::Protocol,
        status: Some(201),
        attempts,
        message: format!("registry response is not UTF-8: {error}"),
    })?;
    let value = parse_json(text).map_err(|error| PublishError {
        kind: PublishErrorKind::Protocol,
        status: Some(201),
        attempts,
        message: format!("invalid registry publish response: {error}"),
    })?;
    let object = value.as_object().ok_or_else(|| PublishError {
        kind: PublishErrorKind::Protocol,
        status: Some(201),
        attempts,
        message: "registry publish response must be an object".into(),
    })?;
    let string_field = |key: &str| {
        object.get(key).and_then(Json::as_str).ok_or_else(|| PublishError {
            kind: PublishErrorKind::Protocol,
            status: Some(201),
            attempts,
            message: format!("registry publish response missing string `{key}`"),
        })
    };
    let status = string_field("status")?;
    let name = string_field("name")?;
    let version = string_field("version")?;
    let checksum = string_field("checksum")?;
    if status != "published"
        || name != preview.package
        || version != preview.version
        || normalize_checksum(checksum) != normalize_checksum(&preview.checksum)
    {
        return Err(PublishError {
            kind: PublishErrorKind::Protocol,
            status: Some(201),
            attempts,
            message: "registry publish receipt does not match the submitted package".into(),
        });
    }
    Ok(PublishReceipt {
        name: name.into(),
        version: version.into(),
        checksum: checksum.into(),
        attempts,
    })
}

fn normalize_checksum(value: &str) -> &str {
    value.strip_prefix("sha256:").unwrap_or(value)
}

fn read_bounded(reader: &mut dyn Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|error| format!("could not read registry response: {error}"))?;
        if count == 0 {
            return Ok(out);
        }
        if out.len().saturating_add(count) > limit {
            return Err(format!("registry response exceeds {limit} bytes"));
        }
        out.extend_from_slice(&chunk[..count]);
    }
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Default on-disk cache: `~/.aura/registry/index` (or `USERPROFILE` on Windows).
pub fn default_index_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".aura")
        .join("registry")
        .join("index")
}

/// `AURA_REGISTRY_INDEX` if set, otherwise [`default_index_path`].
pub fn index_root_from_env() -> PathBuf {
    env::var_os(ENV_REGISTRY_INDEX)
        .map(PathBuf::from)
        .unwrap_or_else(default_index_path)
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Candidate relative paths for a package's `versions.json`.
fn package_versions_rel_paths(name: &str) -> Vec<PathBuf> {
    let file = "versions.json";
    vec![
        // Flat fixture layout (C13i plan / tests)
        PathBuf::from("packages").join(name).join(file),
        // Sparse layout (RFC-005 §6.6.2)
        sparse_package_dir(name).join(file),
    ]
}

/// Cargo-like sparse prefix: `packages/he/ll/hello` for `hello`.
fn sparse_package_dir(name: &str) -> PathBuf {
    let base = PathBuf::from("packages");
    match name.len() {
        0 => base.join("_").join(name),
        1 => base.join("1").join(name),
        2 => base.join("2").join(name),
        3 => base.join("3").join(&name[0..1]).join(name),
        _ => base.join(&name[0..2]).join(&name[2..4]).join(name),
    }
}

fn load_config(path: &Path) -> Result<RegistryConfig, String> {
    if !path.is_file() {
        return Ok(RegistryConfig::default());
    }
    let text =
        fs::read_to_string(path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
    parse_config_json(&text).map_err(|e| format!("error: {}: {e}", path.display()))
}

fn parse_config_json(text: &str) -> Result<RegistryConfig, String> {
    let v = parse_json(text)?;
    let obj = v
        .as_object()
        .ok_or_else(|| "config.json: expected object".to_string())?;
    Ok(RegistryConfig {
        dl: obj.get("dl").and_then(Json::as_str).map(str::to_string),
        api: obj.get("api").and_then(Json::as_str).map(str::to_string),
        github_api: obj
            .get("github_api")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

/// Parse `versions.json`: either a JSON array of version objects, or
/// `{ "versions": [ ... ] }`. Also accepts newline-delimited objects (one per line).
fn parse_versions_json(text: &str) -> Result<Vec<VersionMeta>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    // NDJSON: lines that each start with `{`
    if !trimmed.starts_with('[') && !trimmed.starts_with('{') {
        return Err("versions.json: expected array or object".into());
    }

    // Prefer full JSON parse when the file is a single document.
    if let Ok(v) = parse_json(trimmed) {
        return versions_from_json(v);
    }

    // Fallback: newline-delimited JSON objects
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let v = parse_json(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        out.push(version_meta_from_object(&v).map_err(|e| format!("line {}: {e}", i + 1))?);
    }
    if out.is_empty() {
        return Err("versions.json: no version records".into());
    }
    Ok(out)
}

fn versions_from_json(v: Json) -> Result<Vec<VersionMeta>, String> {
    match v {
        Json::Array(items) => items.iter().map(version_meta_from_object).collect(),
        Json::Object(map) => {
            if let Some(arr) = map.get("versions") {
                let items = arr
                    .as_array()
                    .ok_or_else(|| "versions.json: `versions` must be an array".to_string())?;
                items.iter().map(version_meta_from_object).collect()
            } else {
                // Single version object
                Ok(vec![version_meta_from_object(&Json::Object(map))?])
            }
        }
        _ => Err("versions.json: expected array or object".into()),
    }
}

fn version_meta_from_object(v: &Json) -> Result<VersionMeta, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "version record must be an object".to_string())?;
    let name = obj
        .get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| "version record missing string `name`".to_string())?
        .to_string();
    let vers = obj
        .get("vers")
        .and_then(Json::as_str)
        .ok_or_else(|| "version record missing string `vers`".to_string())?
        .to_string();
    let cksum = obj
        .get("cksum")
        .and_then(Json::as_str)
        .ok_or_else(|| "version record missing string `cksum`".to_string())?
        .to_string();
    let yanked = obj
        .get("yanked")
        .map(|j| j.as_bool().unwrap_or(false))
        .unwrap_or(false);
    let repository = obj
        .get("repository")
        .and_then(Json::as_str)
        .map(str::to_string);
    let targets = obj
        .get("targets")
        .and_then(Json::as_array)
        .map(|items| items.iter().filter_map(Json::as_str).map(str::to_string).collect());
    let targets = targets.or_else(|| {
        obj.get("target")
            .and_then(Json::as_str)
            .map(|target| vec![target.to_string()])
    });
    let min_aura = obj
        .get("min_aura")
        .or_else(|| obj.get("min_toolchain"))
        .and_then(Json::as_str)
        .map(str::to_string);
    let max_aura = obj
        .get("max_aura")
        .or_else(|| obj.get("max_toolchain"))
        .and_then(Json::as_str)
        .map(str::to_string);
    let revoked = obj
        .get("revoked")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let revoke_reason = obj
        .get("revoke_reason")
        .and_then(Json::as_str)
        .map(str::to_string);
    Ok(VersionMeta {
        name,
        vers,
        cksum,
        yanked,
        repository,
        targets,
        min_aura,
        max_aura,
        revoked,
        revoke_reason,
    })
}

// --- Minimal JSON subset parser (objects, arrays, strings, bools, null, numbers) ---

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Object(m) => Some(m),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

fn parse_json(input: &str) -> Result<Json, String> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        i: 0,
    };
    let v = p.parse_value()?;
    p.skip_ws();
    if p.i != p.bytes.len() {
        return Err(format!("trailing junk at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.i) {
            if b.is_ascii_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        Some(b)
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        self.skip_ws();
        match self.bump() {
            Some(b) if b == c => Ok(()),
            Some(b) => Err(format!("expected '{}', got '{}'", c as char, b as char)),
            None => Err(format!("expected '{}', got EOF", c as char)),
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_null(),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(b) => Err(format!("unexpected '{}'", b as char)),
            None => Err("unexpected EOF".into()),
        }
    }

    fn parse_null(&mut self) -> Result<Json, String> {
        for c in b"null" {
            if self.bump() != Some(*c) {
                return Err("invalid null".into());
            }
        }
        Ok(Json::Null)
    }

    fn parse_bool(&mut self) -> Result<Json, String> {
        if self.peek() == Some(b't') {
            for c in b"true" {
                if self.bump() != Some(*c) {
                    return Err("invalid true".into());
                }
            }
            Ok(Json::Bool(true))
        } else {
            for c in b"false" {
                if self.bump() != Some(*c) {
                    return Err("invalid false".into());
                }
            }
            Ok(Json::Bool(false))
        }
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.i;
        let mut end = start;
        if self.bytes.get(end) == Some(&b'-') {
            end += 1;
        }
        while matches!(self.bytes.get(end), Some(b'0'..=b'9')) {
            end += 1;
        }
        if self.bytes.get(end) == Some(&b'.') {
            end += 1;
            while matches!(self.bytes.get(end), Some(b'0'..=b'9')) {
                end += 1;
            }
        }
        if matches!(self.bytes.get(end), Some(b'e') | Some(b'E')) {
            end += 1;
            if matches!(self.bytes.get(end), Some(b'+') | Some(b'-')) {
                end += 1;
            }
            while matches!(self.bytes.get(end), Some(b'0'..=b'9')) {
                end += 1;
            }
        }
        if end == start || (end == start + 1 && self.bytes[start] == b'-') {
            return Err("invalid number".into());
        }
        let s = std::str::from_utf8(&self.bytes[start..end])
            .map_err(|_| "invalid number utf8".to_string())?
            .to_string();
        self.i = end;
        Ok(Json::Number(s))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.bump() != Some(b'"') {
            return Err("expected string".into());
        }
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated string".into()),
                Some(b'"') => return Ok(out),
                Some(b'\\') => {
                    let esc = self
                        .bump()
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut hex = [0u8; 4];
                            for h in &mut hex {
                                *h = self
                                    .bump()
                                    .ok_or_else(|| "bad unicode escape".to_string())?;
                            }
                            let s = std::str::from_utf8(&hex)
                                .map_err(|_| "bad unicode escape".to_string())?;
                            let cp = u32::from_str_radix(s, 16)
                                .map_err(|_| "bad unicode escape".to_string())?;
                            out.push(
                                char::from_u32(cp)
                                    .ok_or_else(|| "bad unicode escape".to_string())?,
                            );
                        }
                        other => {
                            return Err(format!("invalid escape '\\{}'", other as char));
                        }
                    }
                }
                Some(b) => out.push(b as char),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    continue;
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                Some(b) => return Err(format!("expected ',' or ']', got '{}'", b as char)),
                None => return Err("unterminated array".into()),
            }
        }
        Ok(Json::Array(items))
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.expect(b':')?;
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    continue;
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                Some(b) => return Err(format!("expected ',' or '}}', got '{}'", b as char)),
                None => return Err("unterminated object".into()),
            }
        }
        Ok(Json::Object(map))
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::package::archive::{archive_sha256, build_source_archive};
    use crate::package::fetch::{install_from_bytes, read_crate_bytes, sha256_hex};
    use crate::package::publish::PublishPreview;
    use aura_codegen::build_from_file;
    use aura_parser::parse_file;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::process::Command;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn sparse_path_hello() {
        assert_eq!(
            sparse_package_dir("hello"),
            PathBuf::from("packages/he/ll/hello")
        );
    }

    #[test]
    fn parse_versions_array() {
        let v = parse_versions_json(
            r#"[
              {"name":"x","vers":"1.0.0","cksum":"sha256:aa","yanked":false,"repository":"o/r"},
              {"name":"x","vers":"1.0.1","cksum":"bb","yanked":true}
            ]"#,
        )
        .unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].vers, "1.0.0");
        assert!(!v[0].yanked);
        assert_eq!(v[0].repository.as_deref(), Some("o/r"));
        assert!(v[1].yanked);
    }

    #[test]
    fn parse_versions_wrapped() {
        let v =
            parse_versions_json(r#"{ "versions": [ {"name":"y","vers":"0.1.0","cksum":"c"} ] }"#)
                .unwrap();
        assert_eq!(v[0].name, "y");
    }

    fn update_index(contents: &str) -> (RegistryIndex, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "aura-update-index-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let package_dir = root.join("packages").join("demo");
        fs::create_dir_all(&package_dir).unwrap();
        fs::write(package_dir.join("versions.json"), contents).unwrap();
        let index = RegistryIndex::open(&root).unwrap();
        (index, root)
    }

    #[test]
    fn discover_update_selects_compatible_newest_and_renders_reason() {
        let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let (index, root) = update_index(&format!(
            "[{{\"name\":\"demo\",\"vers\":\"1.0.0\",\"cksum\":\"{checksum}\"}},{{\"name\":\"demo\",\"vers\":\"1.1.0\",\"cksum\":\"{checksum}\",\"targets\":[\"linux-amd64\"],\"min_aura\":\"0.1.0\"}},{{\"name\":\"demo\",\"vers\":\"1.2.0\",\"cksum\":\"{checksum}\",\"targets\":[\"linux-amd64\"],\"max_aura\":\"0.2.0\"}}]"
        ));
        let decision = index
            .discover_update("demo", "1.0.0", "0.1.0", "linux-amd64")
            .unwrap();
        match &decision {
            UpdateDecision::Update(candidate) => {
                assert_eq!(candidate.meta.vers, "1.2.0");
                assert_eq!(candidate.target, "linux-amd64");
            }
            other => panic!("expected update, got {other:?}"),
        }
        assert_eq!(decision.code(), "update_available");
        assert!(decision.render_json().contains("1.2.0"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discover_update_reports_unsupported_revoked_and_bad_metadata() {
        let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let (index, root) = update_index(&format!(
            "[{{\"name\":\"demo\",\"vers\":\"2.0.0\",\"cksum\":\"{checksum}\",\"targets\":[\"darwin-arm64\"]}}]"
        ));
        assert!(matches!(
            index.discover_update("demo", "1.0.0", "0.1.0", "linux-amd64").unwrap(),
            UpdateDecision::Unsupported { .. }
        ));
        fs::remove_dir_all(root).unwrap();

        let (index, root) = update_index(&format!(
            "[{{\"name\":\"demo\",\"vers\":\"2.0.0\",\"cksum\":\"{checksum}\",\"targets\":[\"linux-amd64\"],\"revoked\":true,\"revoke_reason\":\"security\"}}]"
        ));
        assert!(matches!(
            index.discover_update("demo", "1.0.0", "0.1.0", "linux-amd64").unwrap(),
            UpdateDecision::Revoked { .. }
        ));
        fs::remove_dir_all(root).unwrap();

        let (index, root) = update_index(
            r#"[{"name":"demo","vers":"2.0.0","cksum":"bad","targets":["linux-amd64"]}]"#,
        );
        let error = index
            .discover_update("demo", "1.0.0", "0.1.0", "linux-amd64")
            .unwrap_err();
        assert!(error.contains("invalid sha256 checksum"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_remote_index_fetches_metadata() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 2048];
                let size = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..size]);
                let body = if request.contains("/config.json") {
                    br#"{"dl":"https://example.test/{name}-{version}.crate","api":"https://example.test"}"#.to_vec()
                } else {
                    br#"[{"name":"tiny","vers":"0.1.0","cksum":"sha256:aa","yanked":false}]"#
                        .to_vec()
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });

        let index = RegistryIndex::open_url(&format!("http://{addr}")).unwrap();
        assert_eq!(index.config().api.as_deref(), Some("https://example.test"));
        assert_eq!(index.package_versions("tiny").unwrap()[0].vers, "0.1.0");
    }

    fn upload_preview() -> PublishPreview {
        let archive = build_source_archive(
            "demo.publish",
            "1.2.3",
            &[("aura.toml".into(), b"[package]\nname=\"demo.publish\"\n".to_vec())],
        )
        .unwrap();
        PublishPreview {
            package: "demo.publish".into(),
            version: "1.2.3".into(),
            archive_name: "demo.publish-1.2.3.crate".into(),
            checksum: archive_sha256(&archive),
            archive,
            signature: None,
            source_entries: vec!["aura.toml".into()],
            dependency_count: 0,
        }
    }

    type FixtureRequests = Arc<Mutex<Vec<Vec<u8>>>>;

    fn upload_fixture(
        responses: Vec<(u16, String)>,
    ) -> (String, FixtureRequests, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                seen.lock().unwrap().push(request);
                let reason = if status == 201 { "Created" } else { "Fixture" };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        (format!("http://{address}"), requests, handle)
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        let header_end;
        loop {
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "fixture received an incomplete request");
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                header_end = end + 4;
                break;
            }
            assert!(bytes.len() < 128 * 1024, "fixture request headers too large");
        }
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while bytes.len() < header_end + length {
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "fixture received an incomplete request body");
            bytes.extend_from_slice(&chunk[..count]);
        }
        bytes
    }

    fn receipt(preview: &PublishPreview) -> String {
        format!(
            "{{\"status\":\"published\",\"name\":\"{}\",\"version\":\"{}\",\"checksum\":\"{}\"}}",
            preview.package, preview.version, preview.checksum
        )
    }

    #[test]
    fn publish_fixture_sends_archive_metadata_and_bearer_auth() {
        let preview = upload_preview();
        let (url, requests, handle) = upload_fixture(vec![(201, receipt(&preview))]);
        let result = publish_upload(&url, Some("fixture-token"), &preview).unwrap();
        handle.join().unwrap();
        assert_eq!(result.attempts, 1);
        let seen = requests.lock().unwrap();
        let raw = &seen[0];
        let request = String::from_utf8_lossy(raw);
        assert!(request.starts_with("POST /api/v1/publish HTTP/1.1"));
        assert!(request.contains("Authorization: Bearer fixture-token"));
        assert!(request.contains(&format!("X-Aura-Package: {}", preview.package)));
        assert!(request.contains(&format!("X-Aura-Version: {}", preview.version)));
        assert!(request.contains(&format!("X-Aura-Sha256: {}", preview.checksum)));
        assert!(raw.ends_with(&preview.archive));
    }

    #[test]
    fn publish_fixture_classifies_auth_and_version_conflict_without_retry() {
        for (status, kind) in [(401, PublishErrorKind::Auth), (409, PublishErrorKind::Conflict)] {
            let preview = upload_preview();
            let (url, requests, handle) = upload_fixture(vec![(status, String::new())]);
            let error = publish_upload(&url, None, &preview).unwrap_err();
            handle.join().unwrap();
            assert_eq!(error.kind, kind);
            assert_eq!(error.attempts, 1);
            assert_eq!(requests.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn publish_fixture_retries_5xx_then_confirms_once() {
        let preview = upload_preview();
        let (url, requests, handle) = upload_fixture(vec![
            (503, "busy".into()),
            (502, "busy".into()),
            (201, receipt(&preview)),
        ]);
        let result = publish_upload(&url, None, &preview).unwrap();
        handle.join().unwrap();
        assert_eq!(result.attempts, 3);
        assert_eq!(requests.lock().unwrap().len(), 3);
    }

    #[test]
    fn publish_fixture_never_claims_success_for_bad_receipt() {
        let preview = upload_preview();
        let (url, _, handle) = upload_fixture(vec![
            (201, "{\"status\":\"published\",\"name\":\"wrong\"}".into()),
        ]);
        let error = publish_upload(&url, None, &preview).unwrap_err();
        handle.join().unwrap();
        assert_eq!(error.kind, PublishErrorKind::Protocol);
        assert_eq!(error.status, Some(201));
    }

    #[test]
    fn publish_fixture_classifies_unreachable_registry_as_indeterminate() {
        let preview = upload_preview();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = publish_upload(&format!("http://{address}"), None, &preview).unwrap_err();
        assert_eq!(error.kind, PublishErrorKind::Indeterminate);
        assert_eq!(error.attempts, 3);
    }

    fn update_fixture(label: &str, versions: &str) -> RegistryIndex {
        let root = std::env::temp_dir().join(format!(
            "aura-u6-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("packages/demo")).unwrap();
        std::fs::write(root.join("packages/demo/versions.json"), versions).unwrap();
        RegistryIndex::open(root).unwrap()
    }

    #[test]
    fn u6_selects_highest_compatible_target_without_fetching() {
        let index = update_fixture(
            "select",
            r#"[
              {"name":"demo","vers":"1.1.0","cksum":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","yanked":false,"targets":["linux-amd64"],"min_aura":"0.1.0"},
              {"name":"demo","vers":"1.2.0","cksum":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","yanked":false,"targets":["darwin-arm64"]},
              {"name":"demo","vers":"1.0.0","cksum":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","yanked":false,"targets":["linux-amd64"]}
            ]"#,
        );
        let decision = index.discover_update("demo", "1.0.0", "0.1.1", "linux-amd64").unwrap();
        match decision {
            UpdateDecision::Update(candidate) => assert_eq!(candidate.meta.vers, "1.1.0"),
            other => panic!("expected update, got {other:?}"),
        }
    }

    #[test]
    fn u6_classifies_no_update_and_unsupported() {
        let index = update_fixture(
            "classify",
            r#"[
              {"name":"demo","vers":"1.1.0","cksum":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","yanked":false,"targets":["darwin-arm64"]}
            ]"#,
        );
        assert!(matches!(
            index.discover_update("demo", "1.1.0", "0.1.1", "linux-amd64").unwrap(),
            UpdateDecision::NoUpdate { .. }
        ));
        assert!(matches!(
            index.discover_update("demo", "1.0.0", "0.1.1", "linux-amd64").unwrap(),
            UpdateDecision::Unsupported { .. }
        ));
    }

    #[test]
    fn u6_classifies_revoked_and_rejects_bad_metadata() {
        let index = update_fixture(
            "revoked",
            r#"[{"name":"demo","vers":"1.1.0","cksum":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","yanked":false,"targets":["linux-amd64"],"revoked":true,"revoke_reason":"compromised"}]"#,
        );
        assert!(matches!(
            index.discover_update("demo", "1.0.0", "0.1.1", "linux-amd64").unwrap(),
            UpdateDecision::Revoked { version, .. } if version == "1.1.0"
        ));
        let bad = update_fixture(
            "bad",
            r#"[{"name":"demo","vers":"1.1.0","cksum":"bad","yanked":false,"targets":["linux-amd64"]}]"#,
        );
        assert!(bad.discover_update("demo", "1.0.0", "0.1.1", "linux-amd64").is_err());
    }

    fn activation_candidate(payload: &[u8], checksum: Option<&str>) -> UpdateCandidate {
        UpdateCandidate {
            meta: VersionMeta {
                name: "aura".into(),
                vers: "0.2.0".into(),
                cksum: checksum
                    .map(str::to_owned)
                    .unwrap_or_else(|| super::super::fetch::sha256_hex(payload)),
                yanked: false,
                repository: None,
                targets: Some(vec!["linux-amd64".into()]),
                min_aura: None,
                max_aura: None,
                revoked: false,
                revoke_reason: None,
            },
            target: "linux-amd64".into(),
            reason: "fixture update".into(),
        }
    }

    fn activation_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "aura-u7-{label}-{}-{}",
            std::process::id(),
            update_nonce()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn u7_filesystem_activation_is_verified_atomic_and_rollbackable() {
        let root = activation_root("filesystem");
        let active = root.join("aura");
        let payload = b"new-aura-binary";
        let source = root.join("download");
        fs::write(&active, b"old-aura-binary").unwrap();
        fs::write(&source, payload).unwrap();
        let candidate = activation_candidate(payload, None);

        let result = activate_update(&candidate, &source.display().to_string(), &active).unwrap();
        assert_eq!(fs::read(&active).unwrap(), payload);
        assert_eq!(fs::read(&result.rollback).unwrap(), b"old-aura-binary");
        let state = fs::read_to_string(&result.state).unwrap();
        assert!(state.contains("signature=deferred"));
        assert!(state.contains("version=0.2.0"));
        assert!(!root.join(".aura.aura-update-staging").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn u7_checksum_and_download_failures_leave_active_untouched() {
        let root = activation_root("failure");
        let active = root.join("aura");
        fs::write(&active, b"old-aura-binary").unwrap();
        let bad_source = root.join("bad-download");
        fs::write(&bad_source, b"tampered").unwrap();
        let bad_checksum = "0000000000000000000000000000000000000000000000000000000000000000";
        let candidate = activation_candidate(b"expected", Some(bad_checksum));

        let error = activate_update(&candidate, &bad_source.display().to_string(), &active).unwrap_err();
        assert!(error.contains("verification failed"));
        assert_eq!(fs::read(&active).unwrap(), b"old-aura-binary");
        assert!(!root.join(".aura.aura-update-state").exists());

        let error = activate_update(&candidate, &root.join("missing").display().to_string(), &active)
            .unwrap_err();
        assert!(error.contains("download failed"));
        assert_eq!(fs::read(&active).unwrap(), b"old-aura-binary");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn u7_http_fixture_downloads_before_activation() {
        let root = activation_root("http");
        let active = root.join("aura");
        let payload = b"http-aura-binary";
        fs::write(&active, b"old-aura-binary").unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let expected = payload.to_vec();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                expected.len()
            )
            .unwrap();
            stream.write_all(&expected).unwrap();
        });
        let candidate = activation_candidate(payload, None);
        let result = activate_update(
            &candidate,
            &format!("http://{address}/aura"),
            &active,
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(fs::read(&active).unwrap(), payload);
        assert!(result.rollback.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn u7_rejects_non_file_active_path_without_mutation() {
        let root = activation_root("permission");
        let active = root.join("active-directory");
        fs::create_dir(&active).unwrap();
        let payload = b"new-aura-binary";
        let source = root.join("download");
        fs::write(&source, payload).unwrap();
        let candidate = activation_candidate(payload, None);
        let error = activate_update(&candidate, &source.display().to_string(), &active).unwrap_err();
        assert!(error.contains("not a regular file"));
        assert!(active.is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn u8_local_registry_release_acceptance_publishes_installs_updates_rolls_back_and_runs() {
        let root = activation_root("release-acceptance");
        let package = root.join("package");
        let registry = root.join("registry");
        let cache = root.join("cache");
        let index = registry.clone();
        fs::create_dir_all(package.join("src")).unwrap();
        fs::create_dir_all(index.join("crates")).unwrap();
        fs::create_dir_all(index.join("packages/release.fixture")).unwrap();
        fs::write(
            package.join("aura.toml"),
            "[package]\nname = \"release.fixture\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::write(
            package.join("src/main.aura"),
            "package release_fixture\nfun main() {\n  println(\"release-v1\")\n}\n",
        )
        .unwrap();

        // U5: publish the deterministic source archive to a local HTTP fixture.
        let preview = crate::package::publish_dry_run(&package).unwrap();
        let published_path = index.join("crates/release.fixture-1.0.0.crate");
        let (url, requests, server) = release_upload_fixture(published_path.clone(), &preview);
        let receipt = publish_upload(&url, Some("u8-fixture-token"), &preview).unwrap();
        server.join().unwrap();
        assert_eq!(receipt.version, "1.0.0");
        let request = String::from_utf8_lossy(&requests.lock().unwrap()[0]).to_string();
        assert!(request.contains("Authorization: Bearer u8-fixture-token"));
        assert!(request.contains("X-Aura-Sha256: "));

        // U3/U5: install the exact bytes acknowledged by the registry and verify
        // the archive checksum before extracting it into the isolated cache.
        let published = read_crate_bytes(&published_path.display().to_string()).unwrap();
        let published_meta = VersionMeta {
            name: "release.fixture".into(),
            vers: "1.0.0".into(),
            cksum: preview.checksum.clone(),
            yanked: false,
            repository: None,
            targets: Some(vec!["linux-amd64".into()]),
            min_aura: None,
            max_aura: None,
            revoked: false,
            revoke_reason: None,
        };
        assert_eq!(sha256_hex(&published), preview.checksum);
        let installed = install_from_bytes(&published_meta, &published, Some(&cache)).unwrap();
        assert!(installed.join("aura.toml").is_file());
        assert!(installed.join("src/main.aura").is_file());

        // Build two native fixture artifacts. The active v1 is replaced by U7's
        // verified v2 payload, then restored from the retained rollback copy.
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        let old_source = parse_file(
            "package release_fixture\nfun main() {\n  println(\"release-v1\")\n}\n",
        )
        .unwrap();
        let new_source = parse_file(
            "package release_fixture\nfun main() {\n  println(\"release-v2\")\n}\n",
        )
        .unwrap();
        let old_binary = root.join("release-v1");
        let new_binary = root.join("release-v2");
        build_from_file(&old_source, &old_binary, &workspace.join("runtime/aura_rt.c")).unwrap();
        build_from_file(&new_source, &new_binary, &workspace.join("runtime/aura_rt.c")).unwrap();
        let new_payload = fs::read(&new_binary).unwrap();
        let new_checksum = sha256_hex(&new_payload);
        let update_payload = index.join("crates/release.fixture-1.1.0.crate");
        fs::write(&update_payload, &new_payload).unwrap();
        fs::write(
            index.join("packages/release.fixture/versions.json"),
            format!(
                "[{{\"name\":\"release.fixture\",\"vers\":\"1.1.0\",\"cksum\":\"{new_checksum}\",\"targets\":[\"linux-amd64\"],\"min_aura\":\"0.1.0\"}}]"
            ),
        )
        .unwrap();

        // U6: discover the compatible target; U7: verify and activate it.
        let registry_index = RegistryIndex::open(&index).unwrap();
        let decision = registry_index
            .discover_update("release.fixture", "1.0.0", "0.1.0", "linux-amd64")
            .unwrap();
        let candidate = match decision {
            UpdateDecision::Update(candidate) => candidate,
            other => panic!("expected U8 update candidate, got {other:?}"),
        };
        let source = registry_index.update_source(&candidate).unwrap();
        let active = root.join("active-aura");
        fs::copy(&old_binary, &active).unwrap();
        assert_eq!(run_fixture_binary(&active), "release-v1");
        let activation = activate_update(&candidate, &source, &active).unwrap();
        assert_eq!(run_fixture_binary(&active), "release-v2");

        // U8 rollback acceptance: atomically restore the retained U7 rollback
        // artifact and prove the previous release still executes.
        let rollback_stage = root.join("active-aura.rollback-staging");
        fs::copy(&activation.rollback, &rollback_stage).unwrap();
        fs::rename(&rollback_stage, &active).unwrap();
        assert_eq!(run_fixture_binary(&active), "release-v1");

        let report = format!(
            "{{\"package\":\"release.fixture\",\"published_version\":\"1.0.0\",\"published_checksum\":\"{}\",\"installed\":true,\"update_version\":\"{}\",\"update_checksum\":\"{}\",\"target\":\"linux-amd64\",\"host\":\"{}-{}\",\"outcome\":\"pass\"}}",
            published_meta.cksum,
            candidate.meta.vers,
            candidate.meta.cksum,
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        if let Ok(path) = std::env::var("AURA_U8_REPORT") {
            fs::write(path, &report).unwrap();
        }
        println!("u8 release acceptance: {report}");
        fs::remove_dir_all(root).unwrap();
    }

    fn release_upload_fixture(
        archive_path: PathBuf,
        preview: &PublishPreview,
    ) -> (String, FixtureRequests, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let receipt = receipt(preview);
        let expected_archive = preview.archive.clone();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap()
                + 4;
            assert_eq!(&request[header_end..], expected_archive.as_slice());
            fs::write(archive_path, &request[header_end..]).unwrap();
            seen.lock().unwrap().push(request);
            write!(
                stream,
                "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{receipt}",
                receipt.len()
            )
            .unwrap();
        });
        (format!("http://{address}"), requests, handle)
    }

    fn run_fixture_binary(path: &Path) -> String {
        let output = Command::new(path).output().unwrap();
        assert!(output.status.success(), "fixture failed: {output:?}");
        String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .to_string()
    }
}
