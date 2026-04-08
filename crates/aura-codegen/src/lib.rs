use anyhow::Result;
use aura_mir::MirProgram;
use std::path::Path;

pub trait Backend {
    /// Compiles the MIR program into an object file (`.o`).
    /// Returns the path to the produced object file.
    fn compile(&self, program: &MirProgram, out_dir: &Path) -> Result<String>;

    /// Optinally emit intermediate representations (IR, Assembly, etc.).
    fn emit_llvm(&self, program: &MirProgram, out_path: &Path) -> Result<()>;
    fn emit_asm(&self, program: &MirProgram, out_path: &Path) -> Result<()>;
}

pub struct Target {
    pub triple: String,
    // Add data layout, CPU, features, etc. as needed
}

impl Target {
    pub fn host() -> Self {
        Self {
            triple: target_lexicon::HOST.to_string(),
        }
    }
}
