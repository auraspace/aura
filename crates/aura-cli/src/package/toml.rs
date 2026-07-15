//! Minimal `aura.toml` parsing.

#[derive(Debug, Clone, Default)]
pub(crate) struct AuraToml {
    pub(crate) package_name: Option<String>,
    pub(crate) bin_name: Option<String>,
    /// Relative path to a source file or directory (default: `src/` or package root).
    pub(crate) bin_path: Option<String>,
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
        let val = parse_toml_string(v.trim())
            .map_err(|e| format!("line {}: {e}", lineno + 1))?;
        match (section.as_str(), key) {
            ("package", "name") => out.package_name = Some(val),
            ("bin", "name") if in_bin => out.bin_name = Some(val),
            ("bin", "path") if in_bin => out.bin_path = Some(val),
            _ => {}
        }
    }
    Ok(out)
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
