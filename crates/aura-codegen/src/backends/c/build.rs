//! Shell out to a C compiler.

use std::path::{Path, PathBuf};

use aura_ast::File;
use aura_ir::LoweredProgram;
use aura_sema::CheckedFile;

use crate::ctx::EmitOptions;
use crate::driver::{Artifact, CBackendDriver};
use crate::error::CodegenError;
use crate::options::{CompileOptions, NativeSource};

pub fn emit_c_from_ast(file: &File) -> Result<String, CodegenError> {
    CBackendDriver::emit(file, EmitOptions::default())
}

pub fn emit_c_from_checked(checked: &CheckedFile) -> String {
    let program = LoweredProgram::from_checked(checked.clone());
    crate::emit::emit_c_with_program(&program, EmitOptions::default())
}

pub fn emit_c_tests_from_ast(file: &File) -> Result<String, CodegenError> {
    CBackendDriver::emit(
        file,
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
}

/// Typecheck + emit C + compile with the system C compiler (`CC` or `cc`).
pub fn build_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(
        file,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions::default(),
    )
}

pub fn build_from_checked(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions::default(),
    )
    .map(Artifact::into_path)
}

pub fn build_from_checked_with_native(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
    native_sources: Vec<NativeSource>,
) -> Result<PathBuf, CodegenError> {
    let options = CompileOptions {
        native_sources,
        ..CompileOptions::default()
    };
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        options,
        EmitOptions::default(),
    )
    .map(Artifact::into_path)
}

pub fn build_from_checked_with_options(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
    mut options: CompileOptions,
    native_sources: Vec<NativeSource>,
) -> Result<PathBuf, CodegenError> {
    options.native_sources = native_sources;
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        options,
        EmitOptions::default(),
    )
    .map(Artifact::into_path)
}

pub fn build_tests_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(
        file,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
}

pub fn build_tests_from_checked(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
    .map(Artifact::into_path)
}

pub fn build_tests_from_checked_with_native(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
    native_sources: Vec<NativeSource>,
    sanitizer: bool,
) -> Result<PathBuf, CodegenError> {
    let mut options = CompileOptions {
        native_sources,
        ..CompileOptions::default()
    };
    options.profile_settings.detector = sanitizer;
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        options,
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
    .map(Artifact::into_path)
}

pub(crate) fn build_from_file_with(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
    compile_options: CompileOptions,
    opts: EmitOptions,
) -> Result<PathBuf, CodegenError> {
    CBackendDriver::build(file, out_bin, runtime_c, compile_options, opts).map(Artifact::into_path)
}

#[cfg(test)]
mod tests {
    include!("build_tests.rs");
}
