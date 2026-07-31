//! Deterministic compiler-host extension points for user derives.
//!
//! The callback API is intentionally AST-only. It is useful for compiler
//! integrations and tests without granting arbitrary source-process access;
//! RFC-010's out-of-process procedural macro ABI remains a separate boundary.

use aura_ast::{ClassDecl, File, FunDecl, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroError {
    pub message: String,
    pub span: Span,
}

/// A compiler-hosted derive implementation.
///
/// Implementations must return synthetic methods whose `span` is the derive
/// invocation span. This preserves expansion-origin metadata and diagnostics.
pub trait UserDerive {
    fn name(&self) -> &str;
    fn expand(&self, input: &ClassDecl) -> Result<Vec<FunDecl>, MacroError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroExpansion {
    pub macro_name: String,
    pub generated_item: String,
    pub invocation_span: Span,
    pub generated_span: Span,
}

/// A deterministic AST macro hook executed before derive expansion.
///
/// This is the compiler-host half of the macro boundary. Package-level token
/// parsing and sandboxed process execution remain outside this trait.
pub trait UserMacro {
    fn name(&self) -> &str;
    fn expand(&self, file: &mut File) -> Result<Vec<MacroExpansion>, MacroError>;
}
