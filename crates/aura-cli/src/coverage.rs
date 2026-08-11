use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageArtifacts {
    pub lcov: PathBuf,
    pub html: PathBuf,
}

pub fn collect(binary: &Path, directory: &Path) -> Result<CoverageArtifacts, String> {
    let profraw = fs::read_dir(directory)
        .map_err(|error| format!("coverage: read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "profraw"))
        .collect::<Vec<_>>();
    if profraw.is_empty() {
        return Err(format!(
            "coverage: test binary produced no .profraw data in {}",
            directory.display()
        ));
    }
    let profdata = directory.join("aura.profdata");
    let lcov = directory.join("aura.lcov");
    let html = directory.join("html");
    let profdata_tool = find_tool("llvm-profdata")?;
    let cov_tool = find_tool("llvm-cov")?;

    let mut merge = Command::new(&profdata_tool);
    merge.arg("merge").arg("-sparse");
    for path in &profraw {
        merge.arg(path);
    }
    merge.arg("-o").arg(&profdata);
    run(&mut merge, "llvm-profdata merge")?;

    let lcov_output = Command::new(&cov_tool)
        .arg("export")
        .arg("-format=lcov")
        .arg(binary)
        .arg(format!("-instr-profile={}", profdata.display()))
        .output()
        .map_err(|error| format!("coverage: failed to run llvm-cov export: {error}"))?;
    if !lcov_output.status.success() {
        return Err(format!(
            "coverage: llvm-cov export failed: {}",
            String::from_utf8_lossy(&lcov_output.stderr).trim()
        ));
    }
    fs::write(&lcov, &lcov_output.stdout)
        .map_err(|error| format!("coverage: write {}: {error}", lcov.display()))?;

    fs::create_dir_all(&html)
        .map_err(|error| format!("coverage: create {}: {error}", html.display()))?;
    let mut show = Command::new(&cov_tool);
    show.arg("show")
        .arg("-format=html")
        .arg(format!("-instr-profile={}", profdata.display()))
        .arg(binary)
        .arg(format!("-output-dir={}", html.display()));
    run(&mut show, "llvm-cov HTML export")?;

    Ok(CoverageArtifacts { lcov, html })
}

fn find_tool(name: &str) -> Result<PathBuf, String> {
    let env_name = format!("AURA_{}", name.replace('-', "_").to_ascii_uppercase());
    if let Some(path) = std::env::var_os(&env_name) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("coverage: {env_name} does not point to a file"));
    }
    if Command::new(name)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
    {
        return Ok(PathBuf::from(name));
    }
    let xcrun = Command::new("xcrun").args(["--find", name]).output();
    if let Ok(output) = xcrun {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }
    Err(format!(
        "coverage: required tool `{name}` not found; install LLVM or set {env_name}"
    ))
}

fn run(command: &mut Command, stage: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("coverage: failed to run {stage}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "coverage: {stage} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::CoverageArtifacts;
    use std::path::PathBuf;

    #[test]
    fn artifacts_keep_stable_report_names() {
        let artifacts = CoverageArtifacts {
            lcov: PathBuf::from("target/aura/coverage/aura.lcov"),
            html: PathBuf::from("target/aura/coverage/html"),
        };
        assert!(artifacts.lcov.ends_with("aura.lcov"));
        assert!(artifacts.html.ends_with("html"));
    }
}
