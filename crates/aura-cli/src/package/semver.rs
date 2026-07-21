//! Semver caret/`^` resolve → exact version pin (C13j).
//!
//! MVP version grammar: `major.minor.patch` with optional `-prerelease`
//! (no build metadata). Version requirements in `aura.toml` are **Cargo-like caret**:
//!
//! | Requirement   | Meaning                                      |
//! |---------------|----------------------------------------------|
//! | `^1.2.3`      | `>=1.2.3, <2.0.0`                            |
//! | `1.2.3`       | same as `^1.2.3` (bare = caret)              |
//! | `1.2`         | `>=1.2.0, <2.0.0`                            |
//! | `1`           | `>=1.0.0, <2.0.0`                            |
//! | `^0.1.2`      | `>=0.1.2, <0.2.0` (0.x: lock minor)          |
//! | `^0.0.3`      | `>=0.0.3, <0.0.4` (0.0.x: lock patch)        |
//!
//! Resolve picks the **highest unyanked** version in the index that matches.

use super::registry::{RegistryIndex, VersionMeta};

/// Parsed semver (MVP: major.minor.patch + optional prerelease identity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Pre-release label without leading `-` (e.g. `alpha`, `alpha.1`).
    pub pre: Option<String>,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: None,
        }
    }

    /// Display form matching index `vers` strings when no pre: `X.Y.Z`.
    #[cfg(test)]
    pub fn to_string_canonical(&self) -> String {
        match &self.pre {
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
            Some(pre) => format!("{}.{}.{}-{}", self.major, self.minor, self.patch, pre),
        }
    }

    /// True if this is a pre-release (`1.0.0-alpha`).
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                // Release > any pre-release of the same numbers.
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(a), Some(b)) => cmp_prerelease(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Dot-separated prerelease: numeric identifiers compare numerically; else ASCII.
fn cmp_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(x), Some(y)) => {
                let o = match (parse_u64_full(x), parse_u64_full(y)) {
                    (Some(nx), Some(ny)) => nx.cmp(&ny),
                    _ => x.cmp(y),
                };
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

fn parse_u64_full(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Inclusive lower bound + exclusive upper bound (caret range).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq {
    /// Original requirement text (for errors).
    pub raw: String,
    pub min: Version,
    /// Exclusive upper bound (no pre-release component).
    pub max_exclusive: Version,
}

impl VersionReq {
    /// Whether `v` satisfies this caret range (Cargo-like prerelease rules).
    ///
    /// Pre-release candidates only match when `min` is itself a pre-release of
    /// the same major.minor.patch and `v >= min` (and still `< max_exclusive`).
    pub fn matches(&self, v: &Version) -> bool {
        if v >= &self.max_exclusive {
            return false;
        }
        if v.is_prerelease() {
            match &self.min.pre {
                None => return false,
                Some(_) => {
                    // Only same major.minor.patch pre-releases can match a pre min.
                    if v.major != self.min.major
                        || v.minor != self.min.minor
                        || v.patch != self.min.patch
                    {
                        return false;
                    }
                    return v >= &self.min;
                }
            }
        }
        v >= &self.min
    }
}

/// Registry lock pin fields derived from resolved metadata (pure; no I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryLockPin {
    pub version: String,
    pub checksum: String,
    pub source: String,
}

impl RegistryLockPin {
    /// Format one `aura.lock` line: `name = { version = "…", checksum = "…", source = "registry" }`.
    pub fn format_lock_line(&self, name: &str) -> String {
        format!(
            "{name} = {{ version = \"{}\", checksum = \"{}\", source = \"{}\" }}",
            self.version, self.checksum, self.source
        )
    }
}

/// Build lock pin fields from resolved index metadata.
pub fn lock_pin_from_meta(meta: &VersionMeta) -> RegistryLockPin {
    RegistryLockPin {
        version: meta.vers.clone(),
        checksum: meta.cksum.clone(),
        source: "registry".into(),
    }
}

/// Parse `1.2.3`, `1.2`, `1`, `0.1.0-alpha` (optional leading `v`).
pub fn parse_version(input: &str) -> Result<Version, String> {
    let s = input.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    if s.is_empty() {
        return Err("empty version string".into());
    }

    let (num_part, pre) = match s.split_once('-') {
        Some((n, p)) => {
            if p.is_empty() {
                return Err(format!("invalid version `{input}`: empty prerelease"));
            }
            if !is_valid_prerelease(p) {
                return Err(format!("invalid version `{input}`: bad prerelease `{p}`"));
            }
            (n, Some(p.to_string()))
        }
        None => (s, None),
    };

    if num_part.is_empty() {
        return Err(format!("invalid version `{input}`"));
    }

    let parts: Vec<&str> = num_part.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(format!(
            "invalid version `{input}`: expected major[.minor[.patch]]"
        ));
    }

    let major = parse_comp(parts[0], input)?;
    let minor = if parts.len() >= 2 {
        parse_comp(parts[1], input)?
    } else {
        0
    };
    let patch = if parts.len() >= 3 {
        parse_comp(parts[2], input)?
    } else {
        0
    };

    Ok(Version {
        major,
        minor,
        patch,
        pre,
    })
}

fn parse_comp(s: &str, full: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid version `{full}`: non-numeric component `{s}`"));
    }
    // Disallow leading zeros except plain "0"
    if s.len() > 1 && s.starts_with('0') {
        return Err(format!("invalid version `{full}`: leading zero in `{s}`"));
    }
    s.parse()
        .map_err(|_| format!("invalid version `{full}`: component overflow `{s}`"))
}

fn is_valid_prerelease(p: &str) -> bool {
    // MVP: alphanumerics, dots, hyphens (no empty segments).
    if p.is_empty() {
        return false;
    }
    p.split('.').all(|seg| {
        !seg.is_empty()
            && seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// Parse a version requirement. Bare versions mean caret (`"1.2"` → `^1.2`).
///
/// Accepts optional leading `^`. Exact equality is not a separate operator in MVP;
/// use a fully pinned resolve only via selecting from the range (highest match).
pub fn parse_req(input: &str) -> Result<VersionReq, String> {
    let raw = input.trim().to_string();
    if raw.is_empty() {
        return Err("empty version requirement".into());
    }
    let body = raw.strip_prefix('^').unwrap_or(raw.as_str()).trim();
    if body.is_empty() {
        return Err(format!("invalid version requirement `{input}`"));
    }
    // Reject other operators for clear errors (MVP is caret-only).
    if body.starts_with('=')
        || body.starts_with('>')
        || body.starts_with('<')
        || body.starts_with('~')
        || body.starts_with('*')
    {
        return Err(format!(
            "unsupported version requirement `{input}` (only caret/`^` or bare versions)"
        ));
    }

    let min = parse_version(body)?;
    let max_exclusive = caret_upper_bound(&min, body);
    Ok(VersionReq {
        raw,
        min,
        max_exclusive,
    })
}

/// Cargo-like caret upper bound from the *written* precision of `body` and zeros.
///
/// For a fully specified version `X.Y.Z` (or with pre), the bound depends on zeros:
/// - major > 0 → `< (major+1).0.0`
/// - major == 0, minor > 0 → `< 0.(minor+1).0`
/// - major == 0, minor == 0 → `< 0.0.(patch+1)`
///
/// Truncated forms fill missing components with 0 before applying the same rules
/// (`1.2` → min 1.2.0 → max 2.0.0; `0.1` → min 0.1.0 → max 0.2.0).
fn caret_upper_bound(min: &Version, body: &str) -> Version {
    let num = body.split_once('-').map(|(n, _)| n).unwrap_or(body);
    let num = num.strip_prefix('v').unwrap_or(num);
    let n_parts = num.split('.').count().min(3);

    // Effective components used for zero-detection (missing → 0).
    let major = min.major;
    let minor = if n_parts >= 2 { min.minor } else { 0 };
    let patch = if n_parts >= 3 { min.patch } else { 0 };

    if major > 0 {
        Version::new(major + 1, 0, 0)
    } else if minor > 0 {
        Version::new(0, minor + 1, 0)
    } else if n_parts >= 3 || patch > 0 {
        // `0.0.Z` or explicit patch
        Version::new(0, 0, patch + 1)
    } else if n_parts == 2 {
        // `0.0` → <0.1.0
        Version::new(0, 1, 0)
    } else {
        // `0` → <1.0.0
        Version::new(1, 0, 0)
    }
}

/// Resolve `req` for package `name` against a local index: highest matching unyanked.
pub fn resolve(name: &str, req: &str, index: &RegistryIndex) -> Result<VersionMeta, String> {
    let requirement = parse_req(req).map_err(|e| {
        format!("error: package `{name}`: invalid version requirement `{req}`: {e}")
    })?;

    let versions = index.package_versions(name)?;
    let mut best: Option<(Version, VersionMeta)> = None;

    for meta in versions {
        if meta.yanked {
            continue;
        }
        let Ok(ver) = parse_version(&meta.vers) else {
            // Skip unparsable index entries rather than failing the whole resolve.
            continue;
        };
        if !requirement.matches(&ver) {
            continue;
        }
        match &best {
            None => best = Some((ver, meta)),
            Some((prev, _)) if ver > *prev => best = Some((ver, meta)),
            _ => {}
        }
    }

    best.map(|(_, meta)| meta).ok_or_else(|| {
        format!(
            "error: no matching version for `{name}` with requirement `{}` \
             (unyanked only; check the registry index)",
            requirement.raw
        )
    })
}

/// Convenience: resolve then produce lock pin fields (version + checksum + source).
#[cfg(test)]
pub fn resolve_lock_pin(
    name: &str,
    req: &str,
    index: &RegistryIndex,
) -> Result<(VersionMeta, RegistryLockPin), String> {
    let meta = resolve(name, req, index)?;
    let pin = lock_pin_from_meta(&meta);
    Ok((meta, pin))
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn parse_full_and_partial() {
        assert_eq!(parse_version("1.2.3").unwrap(), Version::new(1, 2, 3));
        assert_eq!(parse_version("1.2").unwrap(), Version::new(1, 2, 0));
        assert_eq!(parse_version("1").unwrap(), Version::new(1, 0, 0));
        assert_eq!(parse_version("v0.1.0").unwrap(), Version::new(0, 1, 0));
        let pre = parse_version("0.1.0-alpha").unwrap();
        assert_eq!(pre.major, 0);
        assert_eq!(pre.pre.as_deref(), Some("alpha"));
        assert_eq!(pre.to_string_canonical(), "0.1.0-alpha");
        let pre2 = parse_version("1.0.0-alpha.1").unwrap();
        assert_eq!(pre2.pre.as_deref(), Some("alpha.1"));
        assert_eq!(Version::new(1, 2, 3).to_string_canonical(), "1.2.3");
    }

    #[test]
    fn version_ord_prerelease() {
        let a = parse_version("1.0.0-alpha").unwrap();
        let b = parse_version("1.0.0-beta").unwrap();
        let c = parse_version("1.0.0").unwrap();
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    #[test]
    fn caret_1xx() {
        let r = parse_req("^1.2.3").unwrap();
        assert!(r.matches(&Version::new(1, 2, 3)));
        assert!(r.matches(&Version::new(1, 9, 9)));
        assert!(!r.matches(&Version::new(1, 2, 2)));
        assert!(!r.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn bare_means_caret() {
        let a = parse_req("1.2.3").unwrap();
        let b = parse_req("^1.2.3").unwrap();
        assert_eq!(a.min, b.min);
        assert_eq!(a.max_exclusive, b.max_exclusive);
        let r = parse_req("1.2").unwrap();
        assert_eq!(r.min, Version::new(1, 2, 0));
        assert_eq!(r.max_exclusive, Version::new(2, 0, 0));
    }

    #[test]
    fn caret_0x_locks_minor() {
        let r = parse_req("^0.1.2").unwrap();
        assert!(r.matches(&Version::new(0, 1, 2)));
        assert!(r.matches(&Version::new(0, 1, 9)));
        assert!(!r.matches(&Version::new(0, 1, 1)));
        assert!(!r.matches(&Version::new(0, 2, 0)));
    }

    #[test]
    fn caret_00x_locks_patch() {
        let r = parse_req("^0.0.3").unwrap();
        assert!(r.matches(&Version::new(0, 0, 3)));
        assert!(!r.matches(&Version::new(0, 0, 4)));
        assert!(!r.matches(&Version::new(0, 0, 2)));
    }

    #[test]
    fn prerelease_not_matched_by_release_req() {
        let r = parse_req("^1.0.0").unwrap();
        let pre = parse_version("1.0.0-alpha").unwrap();
        assert!(!r.matches(&pre));
        let r2 = parse_req("^1.0.0-alpha").unwrap();
        assert!(r2.matches(&pre));
        assert!(r2.matches(&parse_version("1.0.0-beta").unwrap()));
        assert!(r2.matches(&Version::new(1, 0, 0))); // release still matches >= min
        assert!(!r2.matches(&parse_version("1.0.1-alpha").unwrap()));
    }

    #[test]
    fn lock_pin_format() {
        let meta = VersionMeta {
            name: "hello".into(),
            vers: "1.1.0".into(),
            cksum: "sha256:bb".into(),
            yanked: false,
            repository: None,
        };
        let pin = lock_pin_from_meta(&meta);
        assert_eq!(pin.source, "registry");
        assert_eq!(
            pin.format_lock_line("hello"),
            r#"hello = { version = "1.1.0", checksum = "sha256:bb", source = "registry" }"#
        );
    }
}
