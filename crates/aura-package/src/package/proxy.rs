//! Bounded read-through cache for the Aura origin object protocol.

use super::fetch::read_crate_bytes_bounded;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const MAX_PROXY_OBJECT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ProxyReadThrough {
    upstream: String,
    cache_root: PathBuf,
    max_object_bytes: usize,
}

impl ProxyReadThrough {
    pub fn new(upstream: &str, cache_root: impl AsRef<Path>) -> Result<Self, String> {
        let upstream = upstream.trim_end_matches('/');
        if !(upstream.starts_with("https://") || (cfg!(test) && upstream.starts_with("http://"))) {
            return Err("error: proxy upstream must use HTTPS".into());
        }
        if upstream.is_empty() {
            return Err("error: proxy upstream is empty".into());
        }
        Ok(Self {
            upstream: upstream.into(),
            cache_root: cache_root.as_ref().to_path_buf(),
            max_object_bytes: MAX_PROXY_OBJECT_BYTES,
        })
    }

    pub fn with_max_object_bytes(mut self, limit: usize) -> Result<Self, String> {
        if limit == 0 || limit > MAX_PROXY_OBJECT_BYTES {
            return Err(format!(
                "error: proxy object limit must be between 1 and {MAX_PROXY_OBJECT_BYTES} bytes"
            ));
        }
        self.max_object_bytes = limit;
        Ok(self)
    }

    pub fn read(&self, module: &str, object: &str) -> Result<Vec<u8>, String> {
        validate_component_path(module, "module")?;
        validate_object(object)?;
        let relative = Path::new(module).join(object);
        let cached = self.cache_root.join(&relative);
        if cached.is_file() {
            return fs::read(&cached)
                .map_err(|error| format!("error: read cached proxy object: {error}"));
        }
        let url = format!("{}/{}/{}", self.upstream, module.trim_matches('/'), object);
        let bytes = read_crate_bytes_bounded(&url, self.max_object_bytes)?;
        if let Some(parent) = cached.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("error: create proxy cache directory: {error}"))?;
        }
        let temporary = cached.with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("error: create proxy cache object: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_data())
            .map_err(|error| format!("error: write proxy cache object: {error}"))?;
        fs::rename(&temporary, &cached)
            .or_else(|error| {
                let _ = fs::remove_file(&temporary);
                if cached.is_file() {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|error| format!("error: install proxy cache object: {error}"))?;
        Ok(bytes)
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }
}

fn validate_component_path(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.starts_with('/') || value.contains('\\') {
        return Err(format!("error: invalid proxy {field}"));
    }
    for component in Path::new(value).components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!("error: invalid proxy {field}"));
        }
    }
    Ok(())
}

fn validate_object(object: &str) -> Result<(), String> {
    if !object.starts_with("@v/")
        || object.contains('\\')
        || object
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || object.matches('/').count() != 1
    {
        return Err("error: invalid proxy object; expected @v/<name>".into());
    }
    let name = &object[3..];
    if name == "list" || name.ends_with(".info") || name.ends_with(".mod") || name.ends_with(".zip")
    {
        Ok(())
    } else {
        Err("error: unsupported proxy object".into())
    }
}

#[cfg(test)]
mod tests {
    use super::ProxyReadThrough;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn rejects_unsafe_protocol_paths_and_limits() {
        assert!(ProxyReadThrough::new("ftp://proxy.example", "/tmp/cache").is_err());
        assert!(ProxyReadThrough::new("https://proxy.example", "/tmp/cache")
            .unwrap()
            .read("../escape", "@v/list")
            .is_err());
        assert!(ProxyReadThrough::new("https://proxy.example", "/tmp/cache")
            .unwrap()
            .with_max_object_bytes(0)
            .is_err());
    }

    #[test]
    fn reads_existing_cached_objects_without_network() {
        let root = std::env::temp_dir().join(format!("aura-proxy-cache-{}", std::process::id()));
        let object = root.join("example/pkg/@v/list");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(object.parent().unwrap()).unwrap();
        fs::write(&object, b"1.0.0\n").unwrap();
        let proxy = ProxyReadThrough::new("https://proxy.example", &root).unwrap();
        assert_eq!(proxy.read("example/pkg", "@v/list").unwrap(), b"1.0.0\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn misses_are_fetched_and_atomically_cached() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n1.0.0\n",
                )
                .unwrap();
        });
        let root = std::env::temp_dir().join(format!("aura-proxy-fetch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let proxy = ProxyReadThrough::new(&format!("http://{address}"), &root).unwrap();
        assert_eq!(proxy.read("example/pkg", "@v/list").unwrap(), b"1.0.0\n");
        assert_eq!(
            fs::read(root.join("example/pkg/@v/list")).unwrap(),
            b"1.0.0\n"
        );
        server.join().unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
