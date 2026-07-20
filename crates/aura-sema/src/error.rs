//! Semantic analysis errors.

use std::fmt;

use aura_ast::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemaError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for SemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for SemaError {}

/// One or more semantic errors (C6h multi-error collect).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemaErrors {
    pub errors: Vec<SemaError>,
}

impl SemaErrors {
    pub fn new(errors: Vec<SemaError>) -> Self {
        Self { errors }
    }

    pub fn single(e: SemaError) -> Self {
        Self { errors: vec![e] }
    }

    /// First error (panics if empty).
    pub fn primary(&self) -> &SemaError {
        self.errors
            .first()
            .expect("SemaErrors must contain at least one error")
    }
}

impl From<SemaError> for SemaErrors {
    fn from(e: SemaError) -> Self {
        Self::single(e)
    }
}

impl From<Vec<SemaError>> for SemaErrors {
    fn from(errors: Vec<SemaError>) -> Self {
        Self { errors }
    }
}

impl fmt::Display for SemaErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SemaErrors {}
