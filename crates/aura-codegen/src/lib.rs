use anyhow::Result;
use aura_mir::MirProgram;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Llvm,
    Clif,
}

impl BackendKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "llvm" => Ok(Self::Llvm),
            "clif" => Ok(Self::Clif),
            other => anyhow::bail!("unknown backend `{other}` (expected `llvm` or `clif`)"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Llvm => "llvm",
            Self::Clif => "clif",
        }
    }
}

pub trait Backend {
    /// Compiles the MIR program into an object file (`.o`).
    /// Returns the path to the produced object file.
    ///
    /// The MVP backend is LLVM; Cranelift remains a placeholder backend crate.
    fn compile(&self, program: &MirProgram, out_dir: &Path) -> Result<PathBuf>;

    /// Optinally emit intermediate representations (IR, Assembly, etc.).
    fn emit_llvm(&self, program: &MirProgram, out_path: &Path) -> Result<()>;
    fn emit_asm(&self, program: &MirProgram, out_path: &Path) -> Result<()>;
}

pub struct Target {
    pub triple: String,
    // Add data layout, CPU, features, etc. as needed
}

impl Target {
    pub const AARCH64_APPLE_DARWIN: &'static str = "aarch64-apple-darwin";
    pub const X86_64_UNKNOWN_LINUX_GNU: &'static str = "x86_64-unknown-linux-gnu";

    pub fn new(triple: impl Into<String>) -> Self {
        Self {
            triple: triple.into(),
        }
    }

    pub fn host() -> Self {
        Self::new(target_lexicon::HOST.to_string())
    }

    pub fn aarch64_apple_darwin() -> Self {
        Self::new(Self::AARCH64_APPLE_DARWIN)
    }

    pub fn x86_64_unknown_linux_gnu() -> Self {
        Self::new(Self::X86_64_UNKNOWN_LINUX_GNU)
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::host()
    }
}
