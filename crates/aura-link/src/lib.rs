use anyhow::{Context, Result};
use std::process::Command;

pub struct Linker {
    pub target_triple: String,
}

impl Linker {
    pub fn new(target_triple: String) -> Self {
        Self { target_triple }
    }

    pub fn link(&self, obj_paths: &[&str], runtime_path: &str, out_path: &str) -> Result<()> {
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
