//! Shell out to a C compiler.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_ast::File;
use aura_sema::check_file;

use crate::ctx::EmitOptions;
use crate::emit::{emit_c, emit_c_with};
use crate::error::CodegenError;

pub fn emit_c_from_ast(file: &File) -> Result<String, CodegenError> {
    let checked = check_file(file)?;
    Ok(emit_c(&checked))
}

pub fn emit_c_tests_from_ast(file: &File) -> Result<String, CodegenError> {
    let checked = check_file(file)?;
    Ok(emit_c_with(&checked, EmitOptions { test: true }))
}

/// Typecheck + emit C + compile with the system C compiler (`CC` or `cc`).
pub fn build_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(file, out_bin, runtime_c, EmitOptions::default())
}

pub fn build_tests_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(file, out_bin, runtime_c, EmitOptions { test: true })
}

pub(crate) fn build_from_file_with(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
    opts: EmitOptions,
) -> Result<PathBuf, CodegenError> {
    let checked = check_file(file)?;
    let c_src = emit_c_with(&checked, opts);

    let parent = out_bin
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent).map_err(|e| CodegenError::Io(e.to_string()))?;

    let c_path = parent.join(format!(
        "{}.aura.c",
        out_bin.file_name().and_then(|s| s.to_str()).unwrap_or("out")
    ));
    fs::write(&c_path, c_src).map_err(|e| CodegenError::Io(e.to_string()))?;

    let compiler = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let status = Command::new(&compiler)
        .arg("-O0")
        .arg("-std=c11")
        .arg(&c_path)
        .arg(runtime_c)
        .arg("-o")
        .arg(out_bin)
        .status()
        .map_err(|e| CodegenError::Compile(format!("failed to spawn {compiler}: {e}")))?;

    if !status.success() {
        return Err(CodegenError::Compile(format!(
            "{compiler} failed with status {status} (source {})",
            c_path.display()
        )));
    }

    Ok(out_bin.to_path_buf())
}
