use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_ir::LoweredProgram;

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
        command.arg(&ir_path);
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/runtime.c");
        let runtime_archive = std::env::var_os("AURA_LLVM_RUNTIME_LIB").map(PathBuf::from);
        let exceptions_header =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/llvm_exceptions.h");
        if let Some(runtime_archive) = runtime_archive {
            if !runtime_archive.is_file() {
                return Err(CodegenError::Configuration(format!(
                    "LLVM runtime archive is missing: {}",
                    runtime_archive.display()
                )));
            }
            command.arg("-x").arg("none");
            command.arg(runtime_archive);
        } else {
            if !runtime.is_file() {
                return Err(CodegenError::Configuration(
                    "LLVM runtime source is missing".into(),
                ));
            }
            if !exceptions_header.is_file() {
                return Err(CodegenError::Configuration(
                    "LLVM exception runtime header is missing".into(),
                ));
            }
            command
                .arg("-x")
                .arg("c")
                .args(["-include", "stdio.h"])
                .args(["-include", "stdlib.h"])
                .args(["-include", "string.h"])
                .args(["-include", "setjmp.h"])
                .args(["-include", "stdint.h"])
                .args(["-include", "stdbool.h"])
                .args(["-include", "errno.h"])
                .arg("-Wno-implicit-function-declaration")
                .arg("-DAURA_LLVM_RUNTIME")
                .arg("-DAURA_RUNTIME_NO_MAIN")
                .arg("-include")
                .arg(exceptions_header)
                .arg(runtime);
        }
        command.arg("-o").arg(out_bin);
        for foreign in program.foreign_libraries() {
            command.arg(format!("-l{}", foreign.library));
        }
        command.arg("-lz");
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
            supports_native_compile: true,
        }
    }

    fn emit_ir(&self, program: &LoweredProgram, _opts: crate::driver::BackendOptions) -> String {
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
