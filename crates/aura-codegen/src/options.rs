//! Backend-neutral compilation choices.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Profile {
    /// Legacy name retained for callers that used the pre-profile API.
    Debug,
    Dev,
    Test,
    Release,
}

impl Profile {
    pub fn optimization_level(self) -> &'static str {
        ProfileSettings::for_profile(self).optimization.flag()
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Debug | Self::Dev => "dev",
            Self::Test => "test",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevel {
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
}

impl OptimizationLevel {
    pub const fn flag(self) -> &'static str {
        match self {
            Self::O0 => "O0",
            Self::O1 => "O1",
            Self::O2 => "O2",
            Self::O3 => "O3",
            Self::Os => "Os",
            Self::Oz => "Oz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lto {
    Off,
    Thin,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicStrategy {
    Unwind,
    Abort,
}

/// Fully normalized settings used by the backend. Manifest inheritance and
/// defaults are resolved before these settings reach codegen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSettings {
    pub optimization: OptimizationLevel,
    pub debug: bool,
    pub lto: Lto,
    pub detector: bool,
    pub panic: PanicStrategy,
    pub backend: Backend,
    /// Optional linker flavor passed through the selected C compiler.
    pub linker: Option<String>,
}

impl ProfileSettings {
    pub const fn for_profile(profile: Profile) -> Self {
        match profile {
            Profile::Debug | Profile::Dev => Self {
                optimization: OptimizationLevel::O0,
                debug: true,
                lto: Lto::Off,
                detector: true,
                panic: PanicStrategy::Unwind,
                backend: Backend::C,
                linker: None,
            },
            Profile::Test => Self {
                optimization: OptimizationLevel::O0,
                debug: true,
                lto: Lto::Off,
                detector: true,
                panic: PanicStrategy::Unwind,
                backend: Backend::C,
                linker: None,
            },
            Profile::Release => Self {
                optimization: OptimizationLevel::O2,
                debug: false,
                lto: Lto::Off,
                detector: false,
                panic: PanicStrategy::Abort,
                backend: Backend::C,
                linker: None,
            },
        }
    }

    pub fn validate(&self) -> Result<(), ProfileSettingsError> {
        if self.backend != Backend::C {
            return Err(ProfileSettingsError::UnsupportedBackend);
        }
        if self
            .linker
            .as_deref()
            .is_some_and(|linker| linker.trim().is_empty())
        {
            return Err(ProfileSettingsError::EmptyLinker);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSettingsError {
    UnsupportedBackend,
    EmptyLinker,
}

impl std::fmt::Display for ProfileSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedBackend => write!(f, "only the C backend is supported"),
            Self::EmptyLinker => write!(f, "linker must not be empty"),
        }
    }
}

impl std::error::Error for ProfileSettingsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAbi {
    AuraRtC,
}

impl RuntimeAbi {
    pub const fn identity(self) -> &'static str {
        match self {
            Self::AuraRtC => crate::runtime_abi::ID,
        }
    }

    pub const fn version(self) -> u32 {
        match self {
            Self::AuraRtC => crate::runtime_abi::VERSION,
        }
    }
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
    pub profile_settings: ProfileSettings,
    pub features: BTreeSet<String>,
    pub runtime_abi: Option<RuntimeAbi>,
    pub output: OutputKind,
    pub diagnostics: DiagnosticMode,
    /// Additional search paths used by F2 foreign libraries (`-L`/`-Wl,-rpath`).
    /// Package tooling may leave this empty and use the host toolchain paths.
    pub foreign_library_paths: Vec<PathBuf>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            backend: Backend::C,
            target: Target::Native,
            profile: Profile::Debug,
            profile_settings: ProfileSettings::for_profile(Profile::Debug),
            features: BTreeSet::new(),
            runtime_abi: Some(RuntimeAbi::AuraRtC),
            output: OutputKind::Executable,
            diagnostics: DiagnosticMode::Human,
            foreign_library_paths: Vec::new(),
        }
    }
}

impl CompileOptions {
    pub fn validate(&self) -> Result<(), OptionsError> {
        self.profile_settings
            .validate()
            .map_err(OptionsError::InvalidProfileSettings)?;
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
            "backend={:?}, target={:?}, profile={:?}/{}, settings={:?}, runtime_abi={:?}, output={:?}, diagnostics={:?}, features={:?}",
            self.backend,
            self.target,
            self.profile,
            self.profile.optimization_level(),
            self.profile_settings,
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
    InvalidProfileSettings(ProfileSettingsError),
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
            Self::InvalidProfileSettings(error) => write!(f, "invalid profile settings: {error}"),
        }
    }
}

impl std::error::Error for OptionsError {}

#[derive(Debug, Default)]
pub struct CompileOptionsBuilder {
    backend: Option<Backend>,
    target: Option<Target>,
    profile: Option<Profile>,
    profile_settings: Option<ProfileSettings>,
    features: BTreeSet<String>,
    runtime_abi: Option<RuntimeAbi>,
    output: Option<OutputKind>,
    diagnostics: Option<DiagnosticMode>,
    foreign_library_paths: Vec<PathBuf>,
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

    pub fn profile_settings(mut self, settings: ProfileSettings) -> Self {
        self.profile_settings = Some(settings);
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

    pub fn foreign_library_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.foreign_library_paths.push(path.into());
        self
    }

    pub fn build(self) -> Result<CompileOptions, OptionsError> {
        let backend = self.backend.ok_or(OptionsError::MissingBackend)?;
        let target = self.target.ok_or(OptionsError::MissingTarget)?;
        let profile = self.profile.ok_or(OptionsError::MissingProfile)?;
        let options = CompileOptions {
            backend,
            target,
            profile,
            profile_settings: self
                .profile_settings
                .unwrap_or_else(|| ProfileSettings::for_profile(profile)),
            features: self.features,
            runtime_abi: self.runtime_abi,
            output: self.output.ok_or(OptionsError::MissingOutput)?,
            diagnostics: self.diagnostics.ok_or(OptionsError::MissingDiagnostics)?,
            foreign_library_paths: self.foreign_library_paths,
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
