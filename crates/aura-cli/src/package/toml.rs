//! Minimal `aura.toml` parsing.

use std::collections::HashMap;

/// One `[dependencies]` entry: path or registry version requirement (C13l).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DepSpec {
    /// Path dependency (`{ path = "…" }` or a non-version bare string).
    Path(String),
    /// Registry version requirement (`"1.2"` / `{ version = "1.2" }`).
    Version(String),
}

impl DepSpec {
    pub(crate) fn as_path(&self) -> Option<&str> {
        match self {
            DepSpec::Path(p) => Some(p.as_str()),
            DepSpec::Version(_) => None,
        }
    }

    pub(crate) fn as_version_req(&self) -> Option<&str> {
        match self {
            DepSpec::Version(v) => Some(v.as_str()),
            DepSpec::Path(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AuraToml {
    pub(crate) package_name: Option<String>,
    pub(crate) bin_name: Option<String>,
    /// Relative path to a source file or directory (default: `src/` or package root).
    pub(crate) bin_path: Option<String>,
    /// Dependencies: path and/or registry version requirements.
    pub(crate) dependencies: HashMap<String, DepSpec>,
}

pub(crate) fn parse_aura_toml(text: &str) -> Result<AuraToml, String> {
    let mut out = AuraToml::default();
    let mut section = String::new();
    let mut in_bin = false;

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if line == "[package]" {
                section = "package".into();
                in_bin = false;
            } else if line == "[[bin]]" {
                section = "bin".into();
                in_bin = true;
            } else if line == "[dependencies]" {
                section = "dependencies".into();
                in_bin = false;
            } else {
                section = "other".into();
                in_bin = false;
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(format!("line {}: expected key = value", lineno + 1));
        };
        let key = k.trim();
        let val_raw = v.trim();
        match section.as_str() {
            "package" if key == "name" => {
                out.package_name = Some(
                    parse_toml_string(val_raw)
                        .map_err(|e| format!("line {}: {e}", lineno + 1))?,
                );
            }
            "bin" if in_bin && key == "name" => {
                out.bin_name = Some(
                    parse_toml_string(val_raw)
                        .map_err(|e| format!("line {}: {e}", lineno + 1))?,
                );
            }
            "bin" if in_bin && key == "path" => {
                out.bin_path = Some(
                    parse_toml_string(val_raw)
                        .map_err(|e| format!("line {}: {e}", lineno + 1))?,
                );
            }
            "dependencies" => {
                let dep = parse_dep_spec(val_raw)
                    .map_err(|e| format!("line {}: {e}", lineno + 1))?;
                out.dependencies.insert(key.to_string(), dep);
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Path, registry version table, or bare string (version-like → registry; else path).
fn parse_dep_spec(v: &str) -> Result<DepSpec, String> {
    let v = v.trim();
    if v.starts_with('{') {
        let inner = v
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or_else(|| format!("invalid dependency table `{v}`"))?
            .trim();
        let mut path: Option<String> = None;
        let mut version: Option<String> = None;
        for part in inner.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((k, val)) = part.split_once('=') else {
                return Err(format!("invalid dependency field `{part}`"));
            };
            match k.trim() {
                "path" => path = Some(parse_toml_string(val.trim())?),
                "version" => version = Some(parse_toml_string(val.trim())?),
                // Ignore unknown keys for forward-compat (features, registry, …).
                _ => {}
            }
        }
        return match (path, version) {
            (Some(p), None) => Ok(DepSpec::Path(p)),
            (None, Some(ver)) => Ok(DepSpec::Version(ver)),
            (Some(_), Some(_)) => Err(
                "dependency table cannot set both `path` and `version` (use one source)".into(),
            ),
            (None, None) => Err(
                "dependency table must include `path` or `version`".into(),
            ),
        };
    }
    let s = parse_toml_string(v)?;
    if looks_like_version_req(&s) {
        Ok(DepSpec::Version(s))
    } else {
        Ok(DepSpec::Path(s))
    }
}

/// Bare `"1.2"`, `"^0.1"`, `"0.1.0"` → registry; `"../math"`, `"vendor/x"` → path.
fn looks_like_version_req(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let body = t.strip_prefix('^').unwrap_or(t);
    let first = match body.chars().next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_digit() {
        return false;
    }
    // Path segments like `1.2/foo` are still paths; version reqs are a single token.
    if body.contains('/') || body.contains('\\') {
        return false;
    }
    true
}

fn parse_toml_string(v: &str) -> Result<String, String> {
    let v = v.trim();
    if let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(inner.to_string());
    }
    if let Some(inner) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(inner.to_string());
    }
    if v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Ok(v.to_string());
    }
    Err(format!("invalid value `{v}`"))
}
