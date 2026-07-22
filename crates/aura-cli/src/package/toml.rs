//! Minimal `aura.toml` parsing.

use aura_codegen::{Backend, Lto, OptimizationLevel, PanicStrategy, Profile, ProfileSettings};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

#[derive(Debug, Clone)]
pub(crate) struct AuraToml {
    pub(crate) package_name: Option<String>,
    pub(crate) bin_name: Option<String>,
    /// Relative path to a source file or directory (default: `src/` or package root).
    pub(crate) bin_path: Option<String>,
    /// Dependencies: path and/or registry version requirements.
    pub(crate) dependencies: HashMap<String, DepSpec>,
    /// Fully normalized build settings for each built-in profile.
    pub(crate) profiles: BTreeMap<Profile, ProfileSettings>,
}

impl Default for AuraToml {
    fn default() -> Self {
        Self {
            package_name: None,
            bin_name: None,
            bin_path: None,
            dependencies: HashMap::new(),
            profiles: normalized_default_profiles(),
        }
    }
}

#[derive(Debug, Default)]
struct ProfileOverride {
    inherits: Option<Profile>,
    optimization: Option<OptimizationLevel>,
    debug: Option<bool>,
    lto: Option<Lto>,
    detector: Option<bool>,
    panic: Option<PanicStrategy>,
    backend: Option<Backend>,
    linker: Option<String>,
}

fn built_in_profiles() -> [Profile; 3] {
    [Profile::Dev, Profile::Test, Profile::Release]
}

fn normalized_default_profiles() -> BTreeMap<Profile, ProfileSettings> {
    built_in_profiles()
        .into_iter()
        .map(|profile| (profile, ProfileSettings::for_profile(profile)))
        .collect()
}

pub(crate) fn parse_aura_toml(text: &str) -> Result<AuraToml, String> {
    let mut out = AuraToml::default();
    let mut section = String::new();
    let mut in_bin = false;
    let mut profile = None;
    let mut profile_overrides: BTreeMap<Profile, ProfileOverride> = BTreeMap::new();
    let mut profile_keys = BTreeSet::new();

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if line == "[package]" {
                section = "package".into();
                in_bin = false;
                profile = None;
            } else if line == "[[bin]]" {
                section = "bin".into();
                in_bin = true;
                profile = None;
            } else if line == "[dependencies]" {
                section = "dependencies".into();
                in_bin = false;
                profile = None;
            } else if let Some(name) = line
                .strip_prefix("[profile.")
                .and_then(|s| s.strip_suffix(']'))
            {
                let selected = match name {
                    "dev" => Profile::Dev,
                    "test" => Profile::Test,
                    "release" => Profile::Release,
                    _ => {
                        return Err(format!(
                            "line {}: unknown profile `{name}` (expected dev, test, or release)",
                            lineno + 1
                        ))
                    }
                };
                section = "profile".into();
                in_bin = false;
                profile = Some(selected);
                profile_overrides.entry(selected).or_default();
            } else {
                section = "other".into();
                in_bin = false;
                profile = None;
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(format!("line {}: expected key = value", lineno + 1));
        };
        let key = k.trim();
        let val_raw = v.trim();
        match section.as_str() {
            "profile" => {
                let selected = profile.expect("profile section must select a profile");
                let key_id = (selected, key.to_string());
                if !profile_keys.insert(key_id) {
                    return Err(format!(
                        "line {}: duplicate profile key `{key}` in `{}`",
                        lineno + 1,
                        profile_section(selected)
                    ));
                }
                let settings = profile_overrides
                    .get_mut(&selected)
                    .expect("profile override created with section");
                match key {
                    "inherits" => settings.inherits = Some(parse_profile(val_raw, lineno)?),
                    "optimization" | "opt-level" => {
                        if settings.optimization.is_some() {
                            return Err(format!(
                                "line {}: conflicting optimization keys in `{}`",
                                lineno + 1,
                                profile_section(selected)
                            ));
                        }
                        settings.optimization = Some(parse_optimization(val_raw, lineno)?);
                    }
                    "debug" | "debug-info" => {
                        if settings.debug.is_some() {
                            return Err(format!(
                                "line {}: conflicting debug keys in `{}`",
                                lineno + 1,
                                profile_section(selected)
                            ));
                        }
                        settings.debug = Some(parse_bool(val_raw, lineno, key)?);
                    }
                    "lto" => settings.lto = Some(parse_lto(val_raw, lineno)?),
                    "detector" | "race-detector" => {
                        if settings.detector.is_some() {
                            return Err(format!(
                                "line {}: conflicting detector keys in `{}`",
                                lineno + 1,
                                profile_section(selected)
                            ));
                        }
                        settings.detector = Some(parse_bool(val_raw, lineno, key)?);
                    }
                    "panic" | "panic-strategy" => {
                        if settings.panic.is_some() {
                            return Err(format!(
                                "line {}: conflicting panic keys in `{}`",
                                lineno + 1,
                                profile_section(selected)
                            ));
                        }
                        settings.panic = Some(parse_panic(val_raw, lineno)?);
                    }
                    "backend" => {
                        let backend = parse_toml_string(val_raw)
                            .map_err(|e| format!("line {}: {e}", lineno + 1))?;
                        settings.backend = Some(match backend.as_str() {
                            "c" => Backend::C,
                            _ => {
                                return Err(format!(
                                    "line {}: unsupported backend `{backend}`",
                                    lineno + 1
                                ))
                            }
                        });
                    }
                    "linker" => {
                        settings.linker = Some(
                            parse_toml_string(val_raw)
                                .map_err(|e| format!("line {}: {e}", lineno + 1))?,
                        )
                    }
                    _ => {
                        return Err(format!(
                            "line {}: unknown key `{key}` in `{}`",
                            lineno + 1,
                            profile_section(selected)
                        ));
                    }
                }
            }
            "package" if key == "name" => {
                out.package_name = Some(
                    parse_toml_string(val_raw).map_err(|e| format!("line {}: {e}", lineno + 1))?,
                );
            }
            "bin" if in_bin && key == "name" => {
                out.bin_name = Some(
                    parse_toml_string(val_raw).map_err(|e| format!("line {}: {e}", lineno + 1))?,
                );
            }
            "bin" if in_bin && key == "path" => {
                out.bin_path = Some(
                    parse_toml_string(val_raw).map_err(|e| format!("line {}: {e}", lineno + 1))?,
                );
            }
            "dependencies" => {
                let dep =
                    parse_dep_spec(val_raw).map_err(|e| format!("line {}: {e}", lineno + 1))?;
                out.dependencies.insert(key.to_string(), dep);
            }
            _ => {}
        }
    }
    out.profiles = resolve_profiles(&profile_overrides)?;
    Ok(out)
}

fn profile_section(profile: Profile) -> &'static str {
    match profile {
        Profile::Debug | Profile::Dev => "profile.dev",
        Profile::Test => "profile.test",
        Profile::Release => "profile.release",
    }
}

fn parse_profile(raw: &str, lineno: usize) -> Result<Profile, String> {
    let value = parse_toml_string(raw).map_err(|e| format!("line {}: {e}", lineno + 1))?;
    match value.as_str() {
        "dev" => Ok(Profile::Dev),
        "test" => Ok(Profile::Test),
        "release" => Ok(Profile::Release),
        _ => Err(format!(
            "line {}: unknown inherited profile `{value}`",
            lineno + 1
        )),
    }
}

fn parse_bool(raw: &str, lineno: usize, key: &str) -> Result<bool, String> {
    match raw.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(format!(
            "line {}: `{key}` must be true or false, got `{value}`",
            lineno + 1
        )),
    }
}

fn parse_optimization(raw: &str, lineno: usize) -> Result<OptimizationLevel, String> {
    let value = parse_toml_string(raw).map_err(|e| format!("line {}: {e}", lineno + 1))?;
    match value.to_ascii_lowercase().as_str() {
        "0" | "o0" => Ok(OptimizationLevel::O0),
        "1" | "o1" => Ok(OptimizationLevel::O1),
        "2" | "o2" => Ok(OptimizationLevel::O2),
        "3" | "o3" => Ok(OptimizationLevel::O3),
        "s" | "os" => Ok(OptimizationLevel::Os),
        "z" | "oz" => Ok(OptimizationLevel::Oz),
        value => Err(format!(
            "line {}: invalid optimization level `{value}` (expected 0, 1, 2, 3, s, or z)",
            lineno + 1
        )),
    }
}

fn parse_lto(raw: &str, lineno: usize) -> Result<Lto, String> {
    let value = parse_toml_string(raw).map_err(|e| format!("line {}: {e}", lineno + 1))?;
    match value.as_str() {
        "off" | "false" => Ok(Lto::Off),
        "thin" => Ok(Lto::Thin),
        "full" | "true" => Ok(Lto::Full),
        _ => Err(format!(
            "line {}: invalid lto `{value}` (expected off, thin, or full)",
            lineno + 1
        )),
    }
}

fn parse_panic(raw: &str, lineno: usize) -> Result<PanicStrategy, String> {
    let value = parse_toml_string(raw).map_err(|e| format!("line {}: {e}", lineno + 1))?;
    match value.as_str() {
        "unwind" => Ok(PanicStrategy::Unwind),
        "abort" => Ok(PanicStrategy::Abort),
        _ => Err(format!(
            "line {}: invalid panic strategy `{value}` (expected unwind or abort)",
            lineno + 1
        )),
    }
}

fn apply_override(settings: &mut ProfileSettings, override_: &ProfileOverride) {
    if let Some(value) = override_.optimization {
        settings.optimization = value;
    }
    if let Some(value) = override_.debug {
        settings.debug = value;
    }
    if let Some(value) = override_.lto {
        settings.lto = value;
    }
    if let Some(value) = override_.detector {
        settings.detector = value;
    }
    if let Some(value) = override_.panic {
        settings.panic = value;
    }
    if let Some(value) = override_.backend {
        settings.backend = value;
    }
    if let Some(value) = &override_.linker {
        settings.linker = Some(value.clone());
    }
}

fn resolve_profiles(
    overrides: &BTreeMap<Profile, ProfileOverride>,
) -> Result<BTreeMap<Profile, ProfileSettings>, String> {
    fn resolve_one(
        profile: Profile,
        overrides: &BTreeMap<Profile, ProfileOverride>,
        resolved: &mut BTreeMap<Profile, ProfileSettings>,
        visiting: &mut BTreeSet<Profile>,
    ) -> Result<ProfileSettings, String> {
        if let Some(settings) = resolved.get(&profile) {
            return Ok(settings.clone());
        }
        if !visiting.insert(profile) {
            return Err(format!(
                "profile inheritance cycle includes `{}`",
                profile_section(profile)
            ));
        }
        let override_ = overrides.get(&profile);
        let mut settings = if let Some(parent) = override_.and_then(|o| o.inherits) {
            resolve_one(parent, overrides, resolved, visiting)?
        } else {
            ProfileSettings::for_profile(profile)
        };
        if let Some(override_) = override_ {
            apply_override(&mut settings, override_);
        }
        settings
            .validate()
            .map_err(|e| format!("{}: {e}", profile_section(profile)))?;
        visiting.remove(&profile);
        resolved.insert(profile, settings.clone());
        Ok(settings)
    }

    let mut resolved = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    for profile in built_in_profiles() {
        resolve_one(profile, overrides, &mut resolved, &mut visiting)?;
    }
    Ok(resolved)
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
            (Some(_), Some(_)) => {
                Err("dependency table cannot set both `path` and `version` (use one source)".into())
            }
            (None, None) => Err("dependency table must include `path` or `version`".into()),
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
    if v.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Ok(v.to_string());
    }
    Err(format!("invalid value `{v}`"))
}
