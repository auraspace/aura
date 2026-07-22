//! Deterministic, content-addressed cache for generated artifacts.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

const SCHEMA: &str = "aura-artifact-cache-v1";
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

/// All inputs that can change a compiled artifact. Fields are serialized in a
/// fixed order so a cache hit is independent of map iteration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactCacheKey {
    pub compiler: String,
    pub backend: String,
    pub abi: String,
    pub target: String,
    pub profile: String,
    pub features: Vec<String>,
    pub source: String,
    pub imports: String,
    pub lockfile: String,
    pub toolchain: String,
}

impl ArtifactCacheKey {
    fn canonical(&self) -> String {
        let mut features = self.features.clone();
        features.sort();
        [
            SCHEMA.to_string(),
            self.compiler.clone(),
            self.backend.clone(),
            self.abi.clone(),
            self.target.clone(),
            self.profile.clone(),
            features.join("\x1f"),
            self.source.clone(),
            self.imports.clone(),
            self.lockfile.clone(),
            self.toolchain.clone(),
        ]
        .into_iter()
        .map(|part| format!("{}:{}", part.len(), part))
        .collect::<Vec<_>>()
        .join("\n")
    }

    pub fn digest(&self) -> String {
        hex_digest(self.canonical().as_bytes())
    }
}

#[derive(Debug)]
pub enum CacheError {
    Io(std::io::Error),
    Corrupt(String),
    UnsafeScope(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cache I/O error: {error}"),
            Self::Corrupt(error) => write!(f, "corrupt cache entry: {error}"),
            Self::UnsafeScope(error) => write!(f, "unsafe cache scope: {error}"),
        }
    }
}

impl std::error::Error for CacheError {}

impl From<std::io::Error> for CacheError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A cache rooted at an explicit project/target/profile scope.
#[derive(Debug, Clone)]
pub struct ArtifactCache {
    root: PathBuf,
}

impl ArtifactCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn paths(&self, key: &ArtifactCacheKey) -> (PathBuf, PathBuf) {
        let digest = key.digest();
        (
            self.root.join(format!("{digest}.artifact")),
            self.root.join(format!("{digest}.meta")),
        )
    }

    pub fn load(&self, key: &ArtifactCacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        let (artifact, metadata) = self.paths(key);
        if !artifact.exists() || !metadata.exists() {
            return Ok(None);
        }
        let metadata_text = fs::read_to_string(&metadata)?;
        let expected_prefix = format!("schema={SCHEMA}\nkey={}\nartifact=", key.digest());
        if !metadata_text.starts_with(&expected_prefix) || !metadata_text.ends_with('\n') {
            self.remove_entry(&artifact, &metadata)?;
            return Ok(None);
        }
        let bytes = fs::read(&artifact)?;
        let expected_artifact = metadata_text
            .strip_prefix(&expected_prefix)
            .and_then(|value| value.strip_suffix('\n'))
            .unwrap_or_default();
        if hex_digest(&bytes) != expected_artifact {
            self.remove_entry(&artifact, &metadata)?;
            return Ok(None);
        }
        Ok(Some(bytes))
    }

    pub fn publish(&self, key: &ArtifactCacheKey, bytes: &[u8]) -> Result<(), CacheError> {
        fs::create_dir_all(&self.root)?;
        let (artifact, metadata) = self.paths(key);
        let nonce = format!(
            "{}.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed),
            key.digest()
        );
        let artifact_tmp = self.root.join(format!("{nonce}.artifact"));
        let metadata_tmp = self.root.join(format!("{nonce}.meta"));
        fs::write(&artifact_tmp, bytes)?;
        fs::write(
            &metadata_tmp,
            format!(
                "schema={SCHEMA}\nkey={}\nartifact={}\n",
                key.digest(),
                hex_digest(bytes)
            ),
        )?;
        if let Err(error) = fs::rename(&artifact_tmp, &artifact) {
            let _ = fs::remove_file(&artifact_tmp);
            let _ = fs::remove_file(&metadata_tmp);
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&metadata_tmp, &metadata) {
            let _ = fs::remove_file(&metadata_tmp);
            let _ = fs::remove_file(&artifact);
            return Err(error.into());
        }
        Ok(())
    }

    /// Remove only artifact-cache entries directly under this explicitly
    /// configured scope. Refuse broad roots so a caller cannot clean a whole
    /// workspace/home directory by accident.
    pub fn clean_scope(&self) -> Result<usize, CacheError> {
        if self.root.as_os_str().is_empty()
            || self.root == Path::new("/")
            || self.root == Path::new(".")
        {
            return Err(CacheError::UnsafeScope(self.root.display().to_string()));
        }
        if !self.root.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            let is_cache_file = path
                .extension()
                .is_some_and(|extension| extension == "artifact" || extension == "meta")
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.ends_with(".tmp.artifact") || name.ends_with(".tmp.meta")
                    });
            if is_cache_file && path.is_file() {
                fs::remove_file(path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn remove_entry(&self, artifact: &Path, metadata: &Path) -> Result<(), CacheError> {
        if artifact.exists() {
            fs::remove_file(artifact)?;
        }
        if metadata.exists() {
            fs::remove_file(metadata)?;
        }
        Ok(())
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ArtifactCache, ArtifactCacheKey};

    fn key(source: &str) -> ArtifactCacheKey {
        ArtifactCacheKey {
            compiler: "cc-1".into(),
            backend: "c".into(),
            abi: "aura-c-abi/1".into(),
            target: "native".into(),
            profile: "dev".into(),
            features: vec!["z".into(), "a".into()],
            source: source.into(),
            imports: "imports".into(),
            lockfile: "lock".into(),
            toolchain: "toolchain".into(),
        }
    }

    #[test]
    fn key_digest_is_order_independent_for_features_and_sensitive_to_source() {
        let first = key("source-a");
        let mut second = first.clone();
        second.features.reverse();
        assert_eq!(first.digest(), second.digest());
        second.source = "source-b".into();
        assert_ne!(first.digest(), second.digest());
    }

    #[test]
    fn cache_publishes_atomically_and_discards_corrupt_entries() {
        let root = std::env::temp_dir().join(format!("aura-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = ArtifactCache::new(&root);
        let key = key("source");
        cache.publish(&key, b"artifact").expect("publish");
        assert_eq!(cache.load(&key).expect("load"), Some(b"artifact".to_vec()));
        std::fs::write(root.join(format!("{}.artifact", key.digest())), b"tampered")
            .expect("tamper");
        assert_eq!(cache.load(&key).expect("corrupt load"), None);
        assert!(!root.join(format!("{}.artifact", key.digest())).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn clean_scope_removes_only_cache_entries() {
        let root = std::env::temp_dir().join(format!("aura-cache-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        let cache = ArtifactCache::new(&root);
        cache.publish(&key("source"), b"artifact").expect("publish");
        std::fs::write(root.join("keep.txt"), b"keep").expect("unrelated file");
        assert_eq!(cache.clean_scope().expect("clean"), 2);
        assert!(root.join("keep.txt").exists());
        assert_eq!(cache.clean_scope().expect("repeat clean"), 0);
        assert!(ArtifactCache::new("/").clean_scope().is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
