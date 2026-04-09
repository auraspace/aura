use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub struct Linker {
    pub target_triple: String,
}

impl Linker {
    pub fn new(target_triple: String) -> Self {
        Self { target_triple }
    }

    pub fn link(&self, obj_paths: &[&Path], runtime_path: &Path, out_path: &Path) -> Result<()> {
        let mut cmd = Command::new("clang");

        // Target triple
        cmd.arg("-target").arg(&self.target_triple);

        // Input objects
        for obj in obj_paths {
            cmd.arg(obj);
        }

        // Runtime
        cmd.arg(runtime_path);

        // Output
        cmd.arg("-o").arg(out_path);

        // System libraries (minimal for now)
        cmd.arg("-lc");

        let status = cmd.status().context("Failed to run clang")?;
        if !status.success() {
            anyhow::bail!("clang failed with status {}", status);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn unique_temp_dir() -> PathBuf {
        let mut dir = env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        dir.push(format!("aura-link-test-{}-{}", std::process::id(), stamp));
        dir
    }

    #[test]
    fn forwards_expected_arguments_to_clang() {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).unwrap();

        let args_path = temp_dir.join("clang-args.txt");
        let clang_path = temp_dir.join("clang");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\nexit 0\n",
            args_path.display()
        );
        fs::write(&clang_path, script).unwrap();

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&clang_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&clang_path, perms).unwrap();
        }

        let old_path = env::var_os("PATH");
        let new_path = match &old_path {
            Some(existing) => format!("{}:{}", temp_dir.display(), existing.to_string_lossy()),
            None => temp_dir.display().to_string(),
        };
        env::set_var("PATH", new_path);

        let linker = Linker::new("aarch64-apple-darwin".to_string());
        let obj_path = temp_dir.join("input.o");
        let runtime_path = temp_dir.join("libruntime.a");
        let out_path = temp_dir.join("a.out");
        let objects = [Path::new(&obj_path)];

        let result = linker.link(&objects, &runtime_path, &out_path);

        if let Some(existing) = old_path {
            env::set_var("PATH", existing);
        } else {
            env::remove_var("PATH");
        }

        result.unwrap();

        let args = fs::read_to_string(args_path).unwrap();
        assert!(args.contains("-target"));
        assert!(args.contains("aarch64-apple-darwin"));
        assert!(args.contains(obj_path.to_str().unwrap()));
        assert!(args.contains(runtime_path.to_str().unwrap()));
        assert!(args.contains("-o"));
        assert!(args.contains(out_path.to_str().unwrap()));
        assert!(args.contains("-lc"));
    }
}
