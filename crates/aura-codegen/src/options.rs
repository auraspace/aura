//! Backend-neutral compilation choices.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Debug,
}

impl Profile {
    pub const fn optimization_level(self) -> &'static str {
        match self {
            Self::Debug => "O0",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAbi {
    AuraRtC,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Executable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticMode {
    Human,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOptions {
    pub backend: Backend,
    pub target: Target,
    pub profile: Profile,
    pub features: BTreeSet<String>,
    pub runtime_abi: Option<RuntimeAbi>,
    pub output: OutputKind,
    pub diagnostics: DiagnosticMode,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            backend: Backend::C,
            target: Target::Native,
            profile: Profile::Debug,
            features: BTreeSet::new(),
            runtime_abi: Some(RuntimeAbi::AuraRtC),
            output: OutputKind::Executable,
            diagnostics: DiagnosticMode::Human,
        }
    }
}

impl CompileOptions {
    pub fn validate(&self) -> Result<(), OptionsError> {
        if self.output == OutputKind::Executable && self.runtime_abi.is_none() {
            return Err(OptionsError::MissingRuntimeAbi {
                output: self.output,
            });
        }

        Ok(())
    }
}

impl std::fmt::Display for CompileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "backend={:?}, target={:?}, profile={:?}/{}, runtime_abi={:?}, output={:?}, diagnostics={:?}, features={:?}",
            self.backend,
            self.target,
            self.profile,
            self.profile.optimization_level(),
            self.runtime_abi,
            self.output,
            self.diagnostics,
            self.features
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsError {
    MissingBackend,
    MissingTarget,
    MissingProfile,
    MissingRuntimeAbi { output: OutputKind },
    MissingOutput,
    MissingDiagnostics,
}

impl std::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBackend => write!(f, "backend is required"),
            Self::MissingTarget => write!(f, "target is required"),
            Self::MissingProfile => write!(f, "profile is required"),
            Self::MissingRuntimeAbi { output } => {
                write!(f, "runtime ABI is required for {output:?} output")
            }
            Self::MissingOutput => write!(f, "output kind is required"),
            Self::MissingDiagnostics => write!(f, "diagnostic mode is required"),
        }
    }
}

impl std::error::Error for OptionsError {}

#[derive(Debug, Default)]
pub struct CompileOptionsBuilder {
    backend: Option<Backend>,
    target: Option<Target>,
    profile: Option<Profile>,
    features: BTreeSet<String>,
    runtime_abi: Option<RuntimeAbi>,
    output: Option<OutputKind>,
    diagnostics: Option<DiagnosticMode>,
}

impl CompileOptionsBuilder {
    pub fn backend(mut self, backend: Backend) -> Self {
        self.backend = Some(backend);
        self
    }

    pub fn target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    pub fn profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn runtime_abi(mut self, runtime_abi: RuntimeAbi) -> Self {
        self.runtime_abi = Some(runtime_abi);
        self
    }

    pub fn output(mut self, output: OutputKind) -> Self {
        self.output = Some(output);
        self
    }

    pub fn diagnostics(mut self, diagnostics: DiagnosticMode) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }

    pub fn feature(mut self, feature: impl Into<String>) -> Self {
        self.features.insert(feature.into());
        self
    }

    pub fn build(self) -> Result<CompileOptions, OptionsError> {
        let options = CompileOptions {
            backend: self.backend.ok_or(OptionsError::MissingBackend)?,
            target: self.target.ok_or(OptionsError::MissingTarget)?,
            profile: self.profile.ok_or(OptionsError::MissingProfile)?,
            features: self.features,
            runtime_abi: self.runtime_abi,
            output: self.output.ok_or(OptionsError::MissingOutput)?,
            diagnostics: self.diagnostics.ok_or(OptionsError::MissingDiagnostics)?,
        };
        options.validate()?;
        Ok(options)
    }
}

impl CompileOptions {
    pub fn builder() -> CompileOptionsBuilder {
        CompileOptionsBuilder::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_preserve_current_c_backend() {
        let options = CompileOptions::default();

        assert_eq!(options.backend, Backend::C);
        assert_eq!(options.target, Target::Native);
        assert_eq!(options.profile, Profile::Debug);
        assert_eq!(options.profile.optimization_level(), "O0");
        assert_eq!(options.runtime_abi, Some(RuntimeAbi::AuraRtC));
        assert_eq!(options.output, OutputKind::Executable);
        assert_eq!(options.diagnostics, DiagnosticMode::Human);
        assert!(options.features.is_empty());
        assert!(options.validate().is_ok());
    }

    #[test]
    fn options_have_printable_debug_identity() {
        let options = CompileOptions::default();
        let debug = format!("{options:?}");
        let display = options.to_string();

        assert!(debug.contains("backend: C"));
        assert!(debug.contains("runtime_abi: Some(AuraRtC)"));
        assert!(display.contains("backend=C"));
        assert!(display.contains("profile=Debug/O0"));
    }

    #[test]
    fn incomplete_or_contradictory_options_are_rejected() {
        assert_eq!(
            CompileOptions::builder().build(),
            Err(OptionsError::MissingBackend)
        );

        let missing_runtime = CompileOptions {
            runtime_abi: None,
            ..CompileOptions::default()
        };
        assert_eq!(
            missing_runtime.validate(),
            Err(OptionsError::MissingRuntimeAbi {
                output: OutputKind::Executable
            })
        );
    }
}
