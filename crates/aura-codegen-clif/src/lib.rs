use anyhow::{bail, Result};
use aura_codegen::Backend;
use aura_mir::MirProgram;
use std::path::Path;

pub struct ClifBackend;

impl ClifBackend {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(&self) -> Result<()> {
        bail!("aura-codegen-clif is a placeholder and is not implemented yet")
    }
}

impl Backend for ClifBackend {
    fn compile(&self, _program: &MirProgram, _out_dir: &Path) -> Result<String> {
        self.unsupported()?;
        unreachable!()
    }

    fn emit_llvm(&self, _program: &MirProgram, _out_path: &Path) -> Result<()> {
        self.unsupported()
    }

    fn emit_asm(&self, _program: &MirProgram, _out_path: &Path) -> Result<()> {
        self.unsupported()
    }
}
