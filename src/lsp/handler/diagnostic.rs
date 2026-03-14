use crate::compiler::frontend::error::Severity;
use crate::compiler::frontend::lexer::Lexer;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::sema::checker::SemanticAnalyzer;
use tower_lsp::lsp_types::*;

pub fn collect_diagnostics(
    lexer: &Lexer,
    parser: &Parser,
    analyzer: &SemanticAnalyzer,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Collect Lexer errors
    for diag in &lexer.diagnostics.diagnostics {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position::new(diag.line as u32 - 1, diag.column as u32 - 1),
                end: Position::new(diag.line as u32 - 1, diag.column as u32),
            },
            severity: Some(match diag.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
                Severity::Info => DiagnosticSeverity::INFORMATION,
            }),
            message: diag.message.clone(),
            ..Default::default()
        });
    }

    // Collect Parser errors
    for diag in &parser.diagnostics.diagnostics {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position::new(diag.line as u32 - 1, diag.column as u32 - 1),
                end: Position::new(diag.line as u32 - 1, diag.column as u32),
            },
            severity: Some(match diag.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
                Severity::Info => DiagnosticSeverity::INFORMATION,
            }),
            message: diag.message.clone(),
            ..Default::default()
        });
    }

    // Collect Semantic errors
    for diag in &analyzer.diagnostics.diagnostics {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position::new(diag.line as u32 - 1, diag.column as u32 - 1),
                end: Position::new(diag.line as u32 - 1, diag.column as u32),
            },
            severity: Some(match diag.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
                Severity::Info => DiagnosticSeverity::INFORMATION,
            }),
            message: diag.message.clone(),
            ..Default::default()
        });
    }

    diagnostics
}
