//! Minimal `aura.toml` parsing.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct AuraToml {
    pub(crate) package_name: Option<String>,
    pub(crate) bin_name: Option<String>,
    /// Relative path to a source file or directory (default: `src/` or package root).
    pub(crate) bin_path: Option<String>,
    /// Path dependencies: import package name → relative path.
    pub(crate) dependencies: HashMap<String, String>,
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
                let path = parse_dep_path(val_raw)
                    .map_err(|e| format!("line {}: {e}", lineno + 1))?;
                out.dependencies.insert(key.to_string(), path);
            }
            _ => {}
        }
    }
    Ok(out)
}

/// `name = { path = "..." }` or `name = "path"` (C3f path deps only).
fn parse_dep_path(v: &str) -> Result<String, String> {
    let v = v.trim();
    if v.starts_with('{') {
        // minimal inline table: { path = "..." }
        let inner = v
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or_else(|| format!("invalid dependency table `{v}`"))?
            .trim();
        for part in inner.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((k, val)) = part.split_once('=') else {
                return Err(format!("invalid dependency field `{part}`"));
            };
            if k.trim() == "path" {
                return parse_toml_string(val.trim());
            }
        }
        return Err("dependency table must include `path`".into());
    }
    parse_toml_string(v)
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
