//! Shared compiler analysis API for batch tools and the language server.
//!
//! This crate owns the boundary consumed by clients. Parsing and semantic
//! implementation details remain in `aura-parser` and `aura-sema`.

pub use aura_parser::{parse_file, ParseError};
pub use aura_sema::{check_file, CheckedFile, SemaError, SemaErrors};

use aura_ast::File;

/// Successful analysis of one source document.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub ast: File,
    pub checked: CheckedFile,
}

/// Failure at one compiler phase while analyzing a source document.
#[derive(Debug)]
pub enum AnalysisError {
    Parse(ParseError),
    Sema(SemaErrors),
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Sema(errors) => write!(f, "{errors}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<ParseError> for AnalysisError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<SemaErrors> for AnalysisError {
    fn from(errors: SemaErrors) -> Self {
        Self::Sema(errors)
    }
}

/// Parse and typecheck one source document through the shared analysis path.
pub fn analyze_file(source: &str) -> Result<Analysis, AnalysisError> {
    let ast = parse_file(source)?;
    let checked = check_file(&ast)?;
    Ok(Analysis { ast, checked })
}

#[cfg(test)]
mod tests {
    use super::{analyze_file, AnalysisError};

    #[test]
    fn analyzes_valid_source() {
        let result = analyze_file("package demo\nfun main() {}\n").unwrap();
        assert_eq!(result.checked.package, "demo");
        assert_eq!(result.checked.functions.len(), 1);
    }

    #[test]
    fn preserves_parse_phase_errors() {
        let error = analyze_file("package").unwrap_err();
        assert!(matches!(error, AnalysisError::Parse(_)));
    }
}
