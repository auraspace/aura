//! Codegen errors.

use aura_sema::SemaError;

#[derive(Debug)]
pub enum CodegenError {
    Sema(SemaError),
    Io(String),
    Compile(String),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::Sema(e) => write!(f, "{e}"),
            CodegenError::Io(e) | CodegenError::Compile(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<SemaError> for CodegenError {
    fn from(e: SemaError) -> Self {
        CodegenError::Sema(e)
    }
}
