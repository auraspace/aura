use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tempfile::TempDir;

pub struct TestRunner {
    pub root_dir: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum Expectation {
    Output(String),
    Error(String),
}

impl TestRunner {
    pub fn new() -> Self {
        let root_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("failed to find workspace root");

        Self { root_dir }
    }

    pub fn run_test(&self, aurac_path: &Path, fixture_path: impl AsRef<Path>) -> Result<()> {
        let fixture_path = self.root_dir.join(fixture_path.as_ref());
        let expectations = self.parse_expectations(&fixture_path)?;

        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        // Build runtime once
        self.build_runtime()?;

        let output = Command::new(aurac_path)
            .arg("run")
            .arg(&fixture_path)
            .current_dir(temp_path)
            .env("AURA_WORKSPACE_ROOT", &self.root_dir)
            .output()
            .context("failed to execute aurac")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        for expectation in expectations {
            match expectation {
                Expectation::Output(expected) => {
                    if !stdout.contains(&expected) {
                        anyhow::bail!(
                            "Expected output not found: '{}'\nSTDOUT:\n{}\nSTDERR:\n{}",
                            expected,
                            stdout,
                            stderr
                        );
                    }
                }
                Expectation::Error(expected) => {
                    if !stderr.contains(&expected) {
                        anyhow::bail!(
                            "Expected error not found: '{}'\nSTDOUT:\n{}\nSTDERR:\n{}",
                            expected,
                            stdout,
                            stderr
                        );
                    }
                }
            }
        }

        if !output.status.success() && !self.has_error_expectation(&fixture_path)? {
            anyhow::bail!(
                "aurac failed unexpectedly\nSTDOUT:\n{}\nSTDERR:\n{}",
                stdout,
                stderr
            );
        }

        Ok(())
    }

    fn parse_expectations(&self, path: &Path) -> Result<Vec<Expectation>> {
        let content = fs::read_to_string(path)?;
        parse_expectations_from_str(&content)
    }

    fn has_error_expectation(&self, path: &Path) -> Result<bool> {
        let expectations = self.parse_expectations(path)?;
        Ok(expectations
            .iter()
            .any(|e| matches!(e, Expectation::Error(_))))
    }

    fn build_runtime(&self) -> Result<()> {
        static RUNTIME_BUILT: OnceLock<()> = OnceLock::new();

        RUNTIME_BUILT.get_or_init(|| {
            let status = Command::new("cargo")
                .args(["build", "-p", "aura-rt", "--locked"])
                .current_dir(&self.root_dir)
                .status()
                .expect("failed to run cargo build for aura-rt");

            assert!(status.success(), "cargo build -p aura-rt failed");
        });

        Ok(())
    }
}

fn parse_expectations_from_str(content: &str) -> Result<Vec<Expectation>> {
    let mut expectations = Vec::new();

    let re_expect = Regex::new(r"//\s*expect:\s*(.*)")?;
    let re_error = Regex::new(r"//\s*expect-error:\s*(.*)")?;

    for line in content.lines() {
        if let Some(cap) = re_expect.captures(line) {
            expectations.push(Expectation::Output(cap[1].trim().to_string()));
        }
        if let Some(cap) = re_error.captures(line) {
            expectations.push(Expectation::Error(cap[1].trim().to_string()));
        }
    }

    Ok(expectations)
}

#[cfg(test)]
mod tests {
    use super::{parse_expectations_from_str, Expectation};

    #[test]
    fn parses_output_and_error_expectations() {
        let content = r#"
// expect: hello world
let x = 1
// expect-error: something went wrong
"#;

        let expectations = parse_expectations_from_str(content).unwrap();

        assert_eq!(
            expectations,
            vec![
                Expectation::Output("hello world".to_string()),
                Expectation::Error("something went wrong".to_string()),
            ]
        );
    }

    #[test]
    fn ignores_non_expectation_comments() {
        let content = r#"
// note: this is just a comment
// expect: ok
// expect-error: fail
"#;

        let expectations = parse_expectations_from_str(content).unwrap();

        assert_eq!(
            expectations,
            vec![
                Expectation::Output("ok".to_string()),
                Expectation::Error("fail".to_string()),
            ]
        );
    }
}

#[macro_export]
macro_rules! aura_test {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let runner = $crate::TestRunner::new();
            let aurac_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_aurac"));
            if let Err(e) = runner.run_test(&aurac_path, $path) {
                panic!("Test '{}' failed: {}", stringify!($name), e);
            }
        }
    };
}
