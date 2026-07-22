//! Registry tarball fetch, sha256 verify, and extract to cache (C13k).
//!
//! Install layout (RFC-005 §6.6.3):
//!
//! ```text
//! <cache>/src/<name>-<version>/   # extracted package root
//! ```
//!
//! Cache root: `AURA_REGISTRY_CACHE` if set, else `~/.aura/registry`.
//!
//! Sources (no live network required for tests):
//! - local filesystem path
//! - `file://` URL
//! - raw bytes via [`install_from_bytes`]
//!
//! HTTP(S) returns a clear error so CI stays offline-green; C13l can plug a
//! downloader later. Download URL template expansion is provided as a thin helper.

use super::registry::VersionMeta;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tar::Archive;

/// Env override for the registry cache root (`src/` lives under this).
pub const ENV_REGISTRY_CACHE: &str = "AURA_REGISTRY_CACHE";
/// Optional bearer token used for registry HTTP(S) requests.
pub const ENV_REGISTRY_TOKEN: &str = "AURA_REGISTRY_TOKEN";

/// Default on-disk cache: `~/.aura/registry` (or `USERPROFILE` on Windows).
pub fn default_cache_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".aura")
        .join("registry")
}

/// `AURA_REGISTRY_CACHE` if set, otherwise [`default_cache_root`].
pub fn cache_root_from_env() -> PathBuf {
    env::var_os(ENV_REGISTRY_CACHE)
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_root)
}

/// Extracted package directory: `<cache>/src/<name>-<version>`.
pub fn package_src_dir(cache_root: &Path, name: &str, version: &str) -> PathBuf {
    cache_root.join("src").join(format!("{name}-{version}"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Strip optional `sha256:` prefix and lowercase hex digits.
pub fn normalize_cksum(cksum: &str) -> String {
    let s = cksum.trim();
    let hex = s
        .strip_prefix("sha256:")
        .or_else(|| s.strip_prefix("SHA256:"))
        .unwrap_or(s)
        .trim();
    hex.to_ascii_lowercase()
}

/// Hex-encoded SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Verify `data` against index/lock checksum (`sha256:…` or bare hex).
pub fn verify_sha256(data: &[u8], expected: &str) -> Result<(), String> {
    let want = normalize_cksum(expected);
    if want.len() != 64 || !want.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "error: invalid sha256 checksum (expected 64 hex digits): {expected}"
        ));
    }
    let got = sha256_hex(data);
    if got != want {
        return Err(format!(
            "error: sha256 mismatch: expected {want}, got {got}"
        ));
    }
    Ok(())
}

/// Expand index `dl` template with version metadata.
///
/// Placeholders: `{name}`, `{version}`, `{vers}`, `{owner}`, `{repo}`.
/// `repository` on meta must be `owner/repo` when owner/repo placeholders appear.
pub fn expand_dl_template(template: &str, meta: &VersionMeta) -> Result<String, String> {
    let mut out = template
        .replace("{name}", &meta.name)
        .replace("{version}", &meta.vers)
        .replace("{vers}", &meta.vers);

    if out.contains("{owner}") || out.contains("{repo}") {
        let repo = meta.repository.as_deref().ok_or_else(|| {
            format!(
                "error: package `{}` version `{}` missing `repository` for download URL",
                meta.name, meta.vers
            )
        })?;
        let (owner, name) = repo.split_once('/').ok_or_else(|| {
            format!(
                "error: package `{}` repository must be `owner/repo`, got `{repo}`",
                meta.name
            )
        })?;
        out = out.replace("{owner}", owner).replace("{repo}", name);
    }
    Ok(out)
}

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const HTTP_ATTEMPTS: usize = 3;

/// Read crate bytes from a local path, `file://`, or HTTP(S) URL.
pub fn read_crate_bytes(source: &str) -> Result<Vec<u8>, String> {
    let source = source.trim();
    if source.starts_with("http://") || source.starts_with("https://") {
        return read_http_bytes(source);
    }
    let path = resolve_local_source(source)?;
    fs::read(&path).map_err(|e| format!("error: read crate {}: {e}", path.display()))
}

fn read_http_bytes(url: &str) -> Result<Vec<u8>, String> {
    let token = env::var(ENV_REGISTRY_TOKEN)
        .ok()
        .filter(|token| !token.trim().is_empty());
    read_http_bytes_with_token(url, token.as_deref())
}

fn read_http_bytes_with_token(url: &str, token: Option<&str>) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(HTTP_TIMEOUT)
        .timeout_read(HTTP_TIMEOUT)
        .timeout_write(HTTP_TIMEOUT)
        .build();
    let mut last_error = None;
    for attempt in 0..HTTP_ATTEMPTS {
        let request = if let Some(token) = token {
            agent
                .get(url)
                .set("Authorization", &format!("Bearer {token}"))
        } else {
            agent.get(url)
        };
        match request.call() {
            Ok(response) => {
                let mut bytes = Vec::new();
                match response.into_reader().read_to_end(&mut bytes) {
                    Ok(_) => return Ok(bytes),
                    Err(error) if attempt + 1 < HTTP_ATTEMPTS => {
                        last_error = Some(error.to_string());
                    }
                    Err(error) => {
                        return Err(format!("error: read HTTPS download {url}: {error}"));
                    }
                }
            }
            Err(ureq::Error::Status(code, _)) if code >= 500 && attempt + 1 < HTTP_ATTEMPTS => {
                last_error = Some(format!("HTTP status {code}"));
            }
            Err(ureq::Error::Transport(error)) if attempt + 1 < HTTP_ATTEMPTS => {
                last_error = Some(error.to_string());
            }
            Err(error) => return Err(format!("error: HTTPS download {url}: {error}")),
        }
    }
    Err(format!(
        "error: HTTPS download {url} failed after {HTTP_ATTEMPTS} attempts: {}",
        last_error.unwrap_or_else(|| "unknown transport error".into())
    ))
}

fn resolve_local_source(source: &str) -> Result<PathBuf, String> {
    let s = source.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        return Err(format!(
            "error: network fetch not enabled in offline registry MVP; \
             use a local path or file:// URL (got {s})"
        ));
    }
    if let Some(rest) = s.strip_prefix("file://") {
        // file:///abs/path or file://localhost/abs/path
        let path_str = if let Some(after_host) = rest.strip_prefix("localhost") {
            after_host
        } else {
            rest
        };
        // On Unix, file:///tmp/x → path "/tmp/x"; file://tmp/x → "/tmp/x" if leading /
        let p = if path_str.starts_with('/') {
            PathBuf::from(path_str)
        } else if cfg!(windows) {
            // file:///C:/... → /C:/... after strip; also file://C:/...
            PathBuf::from(path_str.trim_start_matches('/'))
        } else {
            PathBuf::from(format!("/{path_str}"))
        };
        return Ok(p);
    }
    Ok(PathBuf::from(s))
}

/// Local fixture layout: `<index>/crates/<name>-<version>.crate`.
pub fn local_crate_path(index_root: &Path, name: &str, version: &str) -> Option<PathBuf> {
    let p = index_root
        .join("crates")
        .join(format!("{name}-{version}.crate"));
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

/// Resolve a downloadable/local source for `meta` from the index (offline-friendly).
///
/// Order:
/// 1. `<index>/crates/<name>-<version>.crate` (fixture / pre-seeded)
/// 2. Expanded `config.json` `dl` when it is a local path or `file://` URL
///
/// HTTP(S) templates are fetched when no pre-seeded local fixture is available.
pub fn crate_source_for_meta(
    index_root: &Path,
    dl_template: Option<&str>,
    meta: &VersionMeta,
) -> Result<String, String> {
    if let Some(p) = local_crate_path(index_root, &meta.name, &meta.vers) {
        return Ok(p.display().to_string());
    }
    if let Some(tmpl) = dl_template {
        let url = expand_dl_template(tmpl, meta)?;
        if url.starts_with("file://")
            || (!url.starts_with("http://") && !url.starts_with("https://"))
        {
            return Ok(url);
        }
    }
    if let Some(tmpl) = dl_template {
        let url = expand_dl_template(tmpl, meta)?;
        if url.starts_with("http://") || url.starts_with("https://") {
            return Ok(url);
        }
    }
    Err(format!(
        "error: package `{}-{}` has no registry crate source\n  \
         hint: place `{0}-{1}.crate` under the index `crates/` directory, \
         pre-seed `AURA_REGISTRY_CACHE`, or use a local/file:// download template",
        meta.name, meta.vers
    ))
}

/// True when `<cache>/src/<name>-<version>` already looks installed.
#[cfg(test)]
pub fn is_package_installed(cache_root: &Path, name: &str, version: &str) -> bool {
    is_installed(&package_src_dir(cache_root, name, version))
}

/// Ensure `meta` is installed under the cache. Uses a warm cache when present;
/// otherwise fetches from `source` (required if not installed).
///
/// Returns the extracted package directory path.
pub fn ensure_installed(
    meta: &VersionMeta,
    source: Option<&str>,
    cache_root: Option<&Path>,
) -> Result<PathBuf, String> {
    let root = cache_root
        .map(Path::to_path_buf)
        .unwrap_or_else(cache_root_from_env);
    let dest = package_src_dir(&root, &meta.name, &meta.vers);
    if is_installed(&dest) {
        return Ok(dest);
    }
    let source = source.ok_or_else(|| {
        format!(
            "error: package `{}-{}` is not in the registry cache (`{}`) and no download source is available\n  \
             hint: set `AURA_REGISTRY_CACHE`, pre-fetch the crate, or provide a local `.crate` under the index `crates/` dir",
            meta.name,
            meta.vers,
            dest.display()
        )
    })?;
    fetch_and_install(meta, source, Some(&root))
}

/// Fetch crate from `source`, verify `meta.cksum`, extract into cache.
///
/// `cache_root`: explicit root, or [`cache_root_from_env`] when `None`.
/// Returns the extracted package directory path.
///
/// If the destination already looks installed (contains `aura.toml` or any file),
/// returns it without re-fetching.
pub fn fetch_and_install(
    meta: &VersionMeta,
    source: &str,
    cache_root: Option<&Path>,
) -> Result<PathBuf, String> {
    let root = cache_root
        .map(Path::to_path_buf)
        .unwrap_or_else(cache_root_from_env);
    let dest = package_src_dir(&root, &meta.name, &meta.vers);
    if is_installed(&dest) {
        return Ok(dest);
    }
    let bytes = read_crate_bytes(source)?;
    install_from_bytes(meta, &bytes, Some(&root))
}

/// Verify checksum and extract `bytes` (`.crate` / `.tar.gz`) into the cache.
pub fn install_from_bytes(
    meta: &VersionMeta,
    bytes: &[u8],
    cache_root: Option<&Path>,
) -> Result<PathBuf, String> {
    verify_sha256(bytes, &meta.cksum)?;

    let root = cache_root
        .map(Path::to_path_buf)
        .unwrap_or_else(cache_root_from_env);
    let dest = package_src_dir(&root, &meta.name, &meta.vers);

    if is_installed(&dest) {
        return Ok(dest);
    }

    // Stage into a temp dir next to dest, then rename for near-atomic install.
    let parent = dest
        .parent()
        .ok_or_else(|| "error: invalid package cache path".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("error: create cache dir {}: {e}", parent.display()))?;

    let staging = parent.join(format!(
        ".{}.{}-{}-staging-{}",
        meta.name,
        meta.vers,
        "crate",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)
        .map_err(|e| format!("error: create staging dir {}: {e}", staging.display()))?;

    let extract_result = (|| {
        extract_crate_tarball(bytes, &staging)?;
        let package_root = locate_package_root(&staging, &meta.name, &meta.vers)?;
        // Move package contents into final dest.
        if package_root == staging {
            // Archive had no top-level wrapper; staging is the package root.
            if dest.exists() {
                fs::remove_dir_all(&dest)
                    .map_err(|e| format!("error: replace {}: {e}", dest.display()))?;
            }
            fs::rename(&staging, &dest).map_err(|e| {
                format!(
                    "error: install {} → {}: {e}",
                    staging.display(),
                    dest.display()
                )
            })?;
        } else {
            if dest.exists() {
                fs::remove_dir_all(&dest)
                    .map_err(|e| format!("error: replace {}: {e}", dest.display()))?;
            }
            fs::rename(&package_root, &dest).map_err(|e| {
                format!(
                    "error: install {} → {}: {e}",
                    package_root.display(),
                    dest.display()
                )
            })?;
            let _ = fs::remove_dir_all(&staging);
        }
        Ok(dest.clone())
    })();

    if extract_result.is_err() {
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&dest);
    }
    extract_result
}

fn is_installed(dest: &Path) -> bool {
    if !dest.is_dir() {
        return false;
    }
    // Prefer aura.toml; otherwise any non-empty tree counts as installed.
    if dest.join("aura.toml").is_file() {
        return true;
    }
    fs::read_dir(dest)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
}

/// After extract into `staging`, find the package root directory.
fn locate_package_root(staging: &Path, name: &str, version: &str) -> Result<PathBuf, String> {
    let expected = staging.join(format!("{name}-{version}"));
    if expected.is_dir() {
        return Ok(expected);
    }
    // Single top-level directory (Cargo-style) even if name differs slightly.
    let mut entries = fs::read_dir(staging)
        .map_err(|e| format!("error: read staging {}: {e}", staging.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect::<Vec<_>>();
    if entries.len() == 1 {
        return Ok(entries.remove(0).path());
    }
    // Flat archive: treat staging as package root if it has aura.toml.
    if staging.join("aura.toml").is_file() {
        return Ok(staging.to_path_buf());
    }
    Err(format!(
        "error: crate archive for `{name}-{version}` has no package root directory"
    ))
}

/// Extract a gzipped tar (`.crate` / `.tar.gz`) into `dest_dir`.
fn extract_crate_tarball(bytes: &[u8], dest_dir: &Path) -> Result<(), String> {
    let gz = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(gz);
    archive
        .entries()
        .map_err(|e| format!("error: read crate tar: {e}"))?
        .try_for_each(|entry| {
            let mut entry = entry.map_err(|e| format!("error: crate tar entry: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("error: crate tar path: {e}"))?
                .into_owned();
            validate_tar_path(&path)?;
            let out = dest_dir.join(&path);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("error: create {}: {e}", parent.display()))?;
            }
            if entry.header().entry_type().is_dir() {
                fs::create_dir_all(&out)
                    .map_err(|e| format!("error: create dir {}: {e}", out.display()))?;
            } else {
                let mut file = fs::File::create(&out)
                    .map_err(|e| format!("error: write {}: {e}", out.display()))?;
                std::io::copy(&mut entry, &mut file)
                    .map_err(|e| format!("error: extract {}: {e}", out.display()))?;
            }
            Ok(())
        })
}

/// Reject absolute paths and `..` components (zip-slip).
fn validate_tar_path(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Err(format!(
            "error: crate archive contains absolute path: {}",
            path.display()
        ));
    }
    for c in path.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "error: crate archive path escapes extract dir: {}",
                    path.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "error: crate archive contains unsafe path: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod unit {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(status: &str, body: &[u8], content_length: Option<usize>) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status.to_string();
        let body = body.to_vec();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            let length = content_length.unwrap_or(body.len());
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });
        format!("http://{addr}/crate")
    }

    fn serve_retry() -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for (index, (status, body)) in [("503 Service Unavailable", b"retry".as_slice()),
                                             ("200 OK", b"crate-bytes".as_slice())]
                .into_iter()
                .enumerate()
            {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
                if index == 1 {
                    break;
                }
            }
        });
        format!("http://{addr}/crate")
    }

    fn serve_retry_read() -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for (index, body) in [b"short".as_slice(), b"crate-bytes".as_slice()]
                .into_iter()
                .enumerate()
            {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    if index == 0 { 100 } else { body.len() }
                )
                .unwrap();
                stream.write_all(body).unwrap();
                if index == 1 {
                    break;
                }
            }
        });
        format!("http://{addr}/crate")
    }

    fn serve_auth() -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]);
            let (status, body) = if request
                .lines()
                .any(|line| line == "Authorization: Bearer test-token")
            {
                ("200 OK", b"crate-bytes".as_slice())
            } else {
                ("401 Unauthorized", b"unauthorized".as_slice())
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        format!("http://{addr}/crate")
    }

    #[test]
    fn normalize_cksum_strips_prefix() {
        assert_eq!(normalize_cksum("sha256:AbCd"), "abcd");
        assert_eq!(normalize_cksum("  DEADBEEF  "), "deadbeef");
    }

    #[test]
    fn verify_sha256_empty() {
        // SHA-256 of empty string
        let empty = sha256_hex(b"");
        assert_eq!(
            empty,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        verify_sha256(b"", &format!("sha256:{empty}")).unwrap();
        assert!(verify_sha256(b"x", &format!("sha256:{empty}")).is_err());
    }

    #[test]
    fn expand_dl_fills_placeholders() {
        let meta = VersionMeta {
            name: "hello".into(),
            vers: "1.0.0".into(),
            cksum: "sha256:aa".into(),
            yanked: false,
            repository: Some("auraspace/hello".into()),
        };
        let url = expand_dl_template(
            "https://github.com/{owner}/{repo}/releases/download/v{version}/{name}-{vers}.crate",
            &meta,
        )
        .unwrap();
        assert_eq!(
            url,
            "https://github.com/auraspace/hello/releases/download/v1.0.0/hello-1.0.0.crate"
        );
    }

    #[test]
    fn resolve_file_url() {
        let p = resolve_local_source("file:///tmp/foo.crate").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/foo.crate"));
    }

    #[test]
    fn fetch_http_bytes() {
        let url = serve_once("200 OK", b"crate-bytes", None);
        assert_eq!(read_crate_bytes(&url).unwrap(), b"crate-bytes");
    }

    #[test]
    fn retries_transient_http_status() {
        let url = serve_retry();
        assert_eq!(read_crate_bytes(&url).unwrap(), b"crate-bytes");
    }

    #[test]
    fn retries_interrupted_http_download() {
        let url = serve_retry_read();
        assert_eq!(read_crate_bytes(&url).unwrap(), b"crate-bytes");
    }

    #[test]
    fn sends_optional_bearer_token_without_exposing_it_in_errors() {
        let url = serve_auth();
        assert_eq!(read_http_bytes_with_token(&url, Some("test-token")).unwrap(), b"crate-bytes");
    }

    #[test]
    fn reject_http_error_status() {
        let url = serve_once("404 Not Found", b"missing", None);
        let err = read_crate_bytes(&url).unwrap_err();
        assert!(err.contains("404"), "{err}");
    }

    #[test]
    fn reject_interrupted_http_download() {
        let url = serve_once("200 OK", b"short", Some(100));
        let err = read_crate_bytes(&url).unwrap_err();
        assert!(
            err.contains("HTTPS download") || err.contains("read HTTPS"),
            "{err}"
        );
    }
}
