//! Deterministic preflight validation for backend builds.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::options::{Backend, CompileOptions, OutputKind, Profile, RuntimeAbi, Target};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidationError {
    UnsupportedBackend,
    UnsupportedTarget,
    UnsupportedProfile,
    MissingRuntimeAbi,
    UnsupportedRuntimeAbi,
    EmptyCompiler,
    RuntimePathMissing(PathBuf),
    RuntimePathUnreadable(PathBuf),
    UnsupportedHostTarget {
        host: String,
        alternatives: &'static str,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedBackend => write!(f, "only the C backend is supported"),
            Self::UnsupportedTarget => write!(f, "only the native target is supported"),
            Self::UnsupportedProfile => write!(f, "only the Debug profile is supported"),
            Self::MissingRuntimeAbi => {
                write!(f, "AuraRtC runtime ABI is required for executable output")
            }
            Self::UnsupportedRuntimeAbi => {
                write!(
                    f,
                    "only the AuraRtC runtime ABI is supported for executable output"
                )
            }
            Self::EmptyCompiler => write!(f, "the CC command must not be empty"),
            Self::RuntimePathMissing(path) => {
                write!(f, "runtime source does not exist: {}", path.display())
            }
            Self::RuntimePathUnreadable(path) => {
                write!(
                    f,
                    "runtime source is not a readable file: {}",
                    path.display()
                )
            }
            Self::UnsupportedHostTarget { host, alternatives } => write!(
                f,
                "native target is unavailable on host `{host}`; supported alternatives: {alternatives}"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Resolve the compiler command without probing or invoking the compiler.
pub(crate) fn compiler_command() -> String {
    std::env::var("CC").unwrap_or_else(|_| "cc".to_owned())
}

/// Validate all configuration and filesystem inputs needed by a C executable build.
pub(crate) fn validate_build(
    options: &CompileOptions,
    compiler: &str,
    runtime_c: &Path,
) -> Result<(), ValidationError> {
    if options.backend != Backend::C {
        return Err(ValidationError::UnsupportedBackend);
    }
    if options.target != Target::Native {
        return Err(ValidationError::UnsupportedTarget);
    }
    validate_native_host()?;
    if options.profile != Profile::Debug {
        return Err(ValidationError::UnsupportedProfile);
    }

    if options.output == OutputKind::Executable {
        if options.runtime_abi.is_none() {
            return Err(ValidationError::MissingRuntimeAbi);
        }
        if options.runtime_abi != Some(RuntimeAbi::AuraRtC) {
            return Err(ValidationError::UnsupportedRuntimeAbi);
        }
    }

    if compiler.trim().is_empty() {
        return Err(ValidationError::EmptyCompiler);
    }

    let metadata = fs::metadata(runtime_c).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ValidationError::RuntimePathMissing(runtime_c.to_path_buf())
        } else {
            ValidationError::RuntimePathUnreadable(runtime_c.to_path_buf())
        }
    })?;
    if !metadata.is_file() || File::open(runtime_c).is_err() {
        return Err(ValidationError::RuntimePathUnreadable(
            runtime_c.to_path_buf(),
        ));
    }

    Ok(())
}

/// The alpha's `Native` target is intentionally narrower than “any platform
/// on which a C compiler happens to run”. Keep the supported release matrix in
/// one place so a build cannot be advertised as native on an untested host.
fn validate_native_host() -> Result<(), ValidationError> {
    let host = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let supported = matches!(
        (std::env::consts::OS, std::env::consts::ARCH),
        ("linux", "x86_64") | ("macos", "aarch64") | ("macos", "x86_64")
    );
    if supported {
        Ok(())
    } else {
        Err(ValidationError::UnsupportedHostTarget {
            host,
            alternatives: "linux-x86_64, macos-aarch64, macos-x86_64",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{validate_build, ValidationError};
    use crate::options::{CompileOptions, OptionsError};

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn readable_runtime() -> PathBuf {
        let id = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aura-codegen-validation-{}-{id}.c",
            std::process::id()
        ));
        fs::write(&path, "/* test runtime */").expect("create test runtime");
        path
    }

    #[test]
    fn successful_defaults_are_valid_without_invoking_cc() {
        let runtime = readable_runtime();
        assert!(validate_build(&CompileOptions::default(), "cc", &runtime).is_ok());
        fs::remove_file(runtime).expect("remove test runtime");
    }

    #[test]
    fn invalid_backend_is_rejected_by_the_options_contract() {
        let result = CompileOptions::builder().build();
        assert_eq!(result, Err(OptionsError::MissingBackend));
    }

    #[test]
    fn invalid_target_is_rejected_by_the_options_contract() {
        let result = CompileOptions::builder()
            .backend(crate::options::Backend::C)
            .build();
        assert_eq!(result, Err(OptionsError::MissingTarget));
    }

    #[test]
    fn invalid_profile_is_rejected_by_the_options_contract() {
        let result = CompileOptions::builder()
            .backend(crate::options::Backend::C)
            .target(crate::options::Target::Native)
            .build();
        assert_eq!(result, Err(OptionsError::MissingProfile));
    }

    #[test]
    fn invalid_runtime_is_rejected_before_any_backend_work() {
        let mut options = CompileOptions::default();
        options.runtime_abi = None;
        let runtime = readable_runtime();

        assert_eq!(
            validate_build(&options, "cc", &runtime),
            Err(ValidationError::MissingRuntimeAbi)
        );
        fs::remove_file(runtime).expect("remove test runtime");
    }

    #[test]
    fn empty_compiler_is_rejected_without_invoking_an_external_process() {
        let runtime = readable_runtime();
        assert_eq!(
            validate_build(&CompileOptions::default(), "  ", &runtime),
            Err(ValidationError::EmptyCompiler)
        );
        fs::remove_file(runtime).expect("remove test runtime");
    }

    #[test]
    fn missing_runtime_path_is_rejected() {
        let runtime = std::env::temp_dir().join(format!(
            "aura-codegen-validation-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&runtime);

        assert_eq!(
            validate_build(&CompileOptions::default(), "cc", &runtime),
            Err(ValidationError::RuntimePathMissing(runtime))
        );
    }

    #[test]
    fn native_host_validation_matches_the_alpha_release_matrix() {
        let result = super::validate_native_host();
        let host = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        let supported = matches!(
            (std::env::consts::OS, std::env::consts::ARCH),
            ("linux", "x86_64") | ("macos", "aarch64") | ("macos", "x86_64")
        );
        assert_eq!(
            result.is_ok(),
            supported,
            "unexpected host validation for {host}"
        );
    }
}
