//! Deterministic source archive construction for registry publication.

use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

/// Build a reproducible gzip-compressed tar archive rooted at `name-version/`.
///
/// Entries are sorted by their repository-relative path and receive normalized
/// ownership, mode, and timestamp metadata.  Callers remain responsible for
/// manifest/dependency validation before invoking this publisher primitive.
pub fn build_source_archive(
    name: &str,
    version: &str,
    entries: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, String> {
    validate_component(name, "package name")?;
    validate_component(version, "package version")?;
    if entries.is_empty() {
        return Err("archive must contain at least one source entry".into());
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    for (path, _) in &sorted {
        validate_entry_path(path)?;
    }

    let encoder = GzEncoder::new(Vec::new(), Compression::best());
    let mut archive = Builder::new(encoder);
    let root = format!("{name}-{version}");
    for (path, bytes) in sorted {
        let archive_path = format!("{root}/{path}");
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        archive
            .append_data(&mut header, archive_path, bytes.as_slice())
            .map_err(|error| format!("archive entry failed: {error}"))?;
    }
    let encoder = archive
        .into_inner()
        .map_err(|error| format!("archive finalize failed: {error}"))?;
    encoder
        .finish()
        .map_err(|error| format!("archive compression failed: {error}"))
}

/// Return the lowercase SHA-256 digest used by registry metadata.
pub fn archive_sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_component(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(format!(
            "{label} contains an unsafe path component: {value}"
        ));
    }
    Ok(())
}

fn validate_entry_path(path: &str) -> Result<(), String> {
    let candidate = std::path::Path::new(path);
    if path.is_empty() || candidate.is_absolute() {
        return Err(format!("archive entry path is unsafe: {path}"));
    }
    for component in candidate.components() {
        if matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::RootDir
        ) {
            return Err(format!("archive entry path escapes package root: {path}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::Archive;

    #[test]
    fn archive_is_reproducible_and_sorted() {
        let entries = vec![
            ("src/main.aura".into(), b"fun main() {}\n".to_vec()),
            ("aura.toml".into(), b"package = \"demo\"\n".to_vec()),
        ];
        let first = build_source_archive("demo", "1.0.0", &entries).expect("archive");
        let second = build_source_archive("demo", "1.0.0", &entries).expect("archive");
        assert_eq!(first, second);
        assert_eq!(archive_sha256(&first).len(), 64);

        let mut decoder = GzDecoder::new(first.as_slice());
        let mut tar_bytes = Vec::new();
        decoder.read_to_end(&mut tar_bytes).expect("decompress");
        let mut archive = Archive::new(tar_bytes.as_slice());
        let names = archive
            .entries()
            .expect("entries")
            .map(|entry| {
                entry
                    .expect("entry")
                    .path()
                    .expect("path")
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["demo-1.0.0/aura.toml", "demo-1.0.0/src/main.aura"]
        );
    }

    #[test]
    fn archive_rejects_empty_and_escaping_inputs() {
        assert!(build_source_archive("demo", "1.0.0", &[]).is_err());
        assert!(
            build_source_archive("demo", "1.0.0", &[("../escape".into(), b"bad".to_vec())])
                .is_err()
        );
        assert!(
            build_source_archive("../demo", "1.0.0", &[("main.aura".into(), b"ok".to_vec())])
                .is_err()
        );
    }
}
