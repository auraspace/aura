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
use std::fs;
use std::path::{Path, PathBuf};

use super::fetch::read_crate_bytes;

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
        if !base_url.starts_with("https://") && !(cfg!(test) && base_url.starts_with("http://")) {
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
    Ok(VersionMeta {
        name,
        vers,
        cksum,
        yanked,
        repository,
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
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
}
