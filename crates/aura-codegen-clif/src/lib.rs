use anyhow::{bail, Result};
use aura_codegen::Backend;
use aura_mir::MirProgram;
use std::path::{Path, PathBuf};

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
    fn compile(&self, _program: &MirProgram, _out_dir: &Path) -> Result<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    fn empty_program() -> MirProgram {
        MirProgram {
            functions: Vec::new(),
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            method_slots: Vec::new(),
        }
    }

    #[test]
    fn reports_placeholder_backend() {
        let backend = ClifBackend::new();
        let err = backend
            .emit_asm(&empty_program(), Path::new("."))
            .unwrap_err();

        assert!(err.to_string().contains("placeholder"));
    }
}
