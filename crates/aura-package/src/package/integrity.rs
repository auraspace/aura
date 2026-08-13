//! Append-only checksum transparency storage for package artifacts.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumRecord {
    pub sequence: u64,
    pub package: String,
    pub version: String,
    pub source: String,
    pub checksum: String,
}

#[derive(Debug, Clone)]
pub struct ChecksumDatabase {
    path: PathBuf,
}

impl ChecksumDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("error: create checksum database directory: {error}"))?;
        }
        if !path.exists() {
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(|error| format!("error: create checksum database: {error}"))?;
        }
        Ok(Self { path })
    }

    pub fn records(&self) -> Result<Vec<ChecksumRecord>, String> {
        let text = fs::read_to_string(&self.path)
            .map_err(|error| format!("error: read checksum database: {error}"))?;
        let mut records = Vec::new();
        for (expected_sequence, (line_number, line)) in (1u64..).zip(text.lines().enumerate()) {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 5 {
                return Err(format!(
                    "error: checksum database line {} has {} fields",
                    line_number + 1,
                    fields.len()
                ));
            }
            let sequence = fields[0].parse::<u64>().map_err(|_| {
                format!(
                    "error: checksum database line {} has invalid sequence",
                    line_number + 1
                )
            })?;
            if sequence != expected_sequence {
                return Err(format!(
                    "error: checksum database sequence gap at line {}",
                    line_number + 1
                ));
            }
            let record = ChecksumRecord {
                sequence,
                package: decode_field(fields[1], line_number + 1)?,
                version: decode_field(fields[2], line_number + 1)?,
                source: decode_field(fields[3], line_number + 1)?,
                checksum: validate_checksum(fields[4], line_number + 1)?,
            };
            records.push(record);
        }
        Ok(records)
    }

    pub fn record(
        &self,
        package: &str,
        version: &str,
        source: &str,
        checksum: &str,
    ) -> Result<ChecksumRecord, String> {
        validate_name(package, "package")?;
        validate_name(version, "version")?;
        validate_name(source, "source")?;
        let checksum = validate_checksum(checksum, 0)?;
        let records = self.records()?;
        if let Some(existing) = records
            .iter()
            .find(|entry| entry.package == package && entry.version == version)
        {
            if existing.checksum == checksum && existing.source == source {
                return Ok(existing.clone());
            }
            return Err(format!(
                "error: checksum database refuses conflicting record for {package}@{version}"
            ));
        }
        let sequence = records.len() as u64 + 1;
        let line = format!(
            "{sequence}\t{}\t{}\t{}\t{checksum}\n",
            encode_field(package)?,
            encode_field(version)?,
            encode_field(source)?,
        );
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("error: open checksum database: {error}"))?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.sync_data())
            .map_err(|error| format!("error: append checksum database: {error}"))?;
        Ok(ChecksumRecord {
            sequence,
            package: package.into(),
            version: version.into(),
            source: source.into(),
            checksum,
        })
    }

    pub fn verify(&self, package: &str, version: &str, checksum: &str) -> Result<(), String> {
        let checksum = validate_checksum(checksum, 0)?;
        let record = self
            .records()?
            .into_iter()
            .find(|entry| entry.package == package && entry.version == version)
            .ok_or_else(|| format!("error: no checksum record for {package}@{version}"))?;
        if record.checksum != checksum {
            return Err(format!(
                "error: checksum transparency mismatch for {package}@{version}"
            ));
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn validate_name(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 4096 || value.contains(['\t', '\n', '\r']) {
        return Err(format!("error: invalid checksum database {field}"));
    }
    Ok(())
}

fn encode_field(value: &str) -> Result<String, String> {
    validate_name(value, "field")?;
    Ok(value.replace('\\', "\\\\"))
}

fn decode_field(value: &str, line: usize) -> Result<String, String> {
    if value.contains(['\n', '\r', '\t']) {
        return Err(format!(
            "error: checksum database line {line} has invalid field"
        ));
    }
    Ok(value.replace("\\\\", "\\"))
}

fn validate_checksum(value: &str, line: usize) -> Result<String, String> {
    let normalized = value.strip_prefix("sha256:").unwrap_or(value);
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        if line == 0 {
            return Err("error: checksum database requires a SHA-256 checksum".into());
        }
        return Err(format!(
            "error: checksum database line {line} has invalid SHA-256 checksum"
        ));
    }
    Ok(normalized.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::ChecksumDatabase;
    use std::fs;

    #[test]
    fn records_are_idempotent_and_conflicts_fail_closed() {
        let path = std::env::temp_dir().join(format!(
            "aura-checksum-db-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let db = ChecksumDatabase::open(&path).unwrap();
        let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            db.record("demo", "1.0.0", "git+https://example", checksum)
                .unwrap()
                .sequence,
            1
        );
        assert_eq!(
            db.record("demo", "1.0.0", "git+https://example", checksum)
                .unwrap()
                .sequence,
            1
        );
        assert!(db
            .record("demo", "1.0.0", "git+https://example", &"0".repeat(64))
            .is_err());
        db.verify("demo", "1.0.0", checksum).unwrap();
        let _ = fs::remove_file(path);
    }

    #[test]
    fn tampered_sequence_and_checksum_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("aura-checksum-tamper-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            "2\tdemo\t1.0.0\tgit+https://example\t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
        )
        .unwrap();
        assert!(ChecksumDatabase::open(&path).unwrap().records().is_err());
        let _ = fs::remove_file(path);
    }
}
