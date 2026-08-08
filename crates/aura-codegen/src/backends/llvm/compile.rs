use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_ir::LoweredProgram;

use crate::ctx::EmitOptions;
use crate::driver::{Artifact, Backend, BackendBuildOptions, BackendCapabilities};
use crate::error::CodegenError;

use super::emit::emit_module;

#[derive(Debug, Clone, Copy, Default)]
pub struct LlvmBackend;

impl LlvmBackend {
    pub fn emit_module(program: &LoweredProgram) -> Result<String, CodegenError> {
        emit_module(program)
    }

    pub fn compile(
        program: &LoweredProgram,
        out_bin: &Path,
        options: &BackendBuildOptions,
    ) -> Result<Artifact, CodegenError> {
        let module = emit_module(program)?;
        let parent = out_bin.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| CodegenError::Io(error.to_string()))?;
        let ir_path = llvm_ir_path(parent, out_bin);
        fs::write(&ir_path, module).map_err(|error| CodegenError::Io(error.to_string()))?;

        let clang = std::env::var("AURA_LLVM_CC").unwrap_or_else(|_| "clang".into());
        let mut command = Command::new(&clang);
        command.arg("-x").arg("ir");
        command.arg(format!("-{}", options.optimization.flag()));
        if options.debug {
            command.arg("-g");
        }
        if options.lto != crate::options::Lto::Off {
            command.arg("-flto");
        }
        command.arg(&ir_path).arg("-o").arg(out_bin);
        for foreign in program.foreign_libraries() {
            command.arg(format!("-l{}", foreign.library));
        }
        let output = command
            .output()
            .map_err(|error| CodegenError::Compile(format!("failed to spawn {clang}: {error}")))?;
        if !output.status.success() {
            return Err(CodegenError::Compile(format!(
                "{clang} failed to compile LLVM IR: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(Artifact::from_backend(out_bin.to_path_buf(), options))
    }
}

impl Backend for LlvmBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            requires_complete_mir: true,
            accepts_alpha_source: false,
            requires_c_runtime: false,
            supports_native_compile: true,
        }
    }

    fn emit(&self, program: &LoweredProgram, _opts: EmitOptions) -> String {
        Self::emit_module(program).unwrap_or_else(|error| format!("; LLVM emission error: {error}"))
    }

    fn compile_ir(
        &self,
        program: &LoweredProgram,
        out_bin: &Path,
        options: &BackendBuildOptions,
        _opts: crate::driver::BackendOptions,
    ) -> Result<Artifact, CodegenError> {
        Self::compile(program, out_bin, options)
    }
}

fn llvm_ir_path(parent: &Path, out_bin: &Path) -> PathBuf {
    parent.join(format!(
        "{}.aura.ll",
        out_bin
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("out")
    ))
}
