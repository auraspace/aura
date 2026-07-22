//! Codegen errors.

use aura_sema::{SemaError, SemaErrors};

use crate::validation::ValidationError;

#[derive(Debug)]
pub enum CodegenError {
    Sema(SemaErrors),
    Io(String),
    Configuration(String),
    Compile(String),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::Sema(e) => write!(f, "{e}"),
            CodegenError::Io(e) | CodegenError::Configuration(e) | CodegenError::Compile(e) => {
                write!(f, "{e}")
            }
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<SemaErrors> for CodegenError {
    fn from(e: SemaErrors) -> Self {
        CodegenError::Sema(e)
    }
}

impl From<SemaError> for CodegenError {
    fn from(e: SemaError) -> Self {
        CodegenError::Sema(SemaErrors::single(e))
    }
}

impl From<ValidationError> for CodegenError {
    fn from(error: ValidationError) -> Self {
        CodegenError::Configuration(error.to_string())
    }
}
