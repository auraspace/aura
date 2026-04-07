use std::fs;
use std::io;
use std::path::Path;

use aura_diagnostics::Diagnostic;

pub mod modules;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckOutput {
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn check_file(path: impl AsRef<Path>) -> io::Result<CheckOutput> {
    let source = fs::read_to_string(path)?;
    let parsed = aura_parser::parse_program(&source);
    Ok(CheckOutput {
        source,
        diagnostics: parsed.errors,
    })
}
