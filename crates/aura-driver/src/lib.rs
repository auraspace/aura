use std::fs;
use std::io;
use std::path::Path;

use aura_diagnostics::Diagnostic;

pub mod modules;
pub mod resolve;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckOutput {
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn check_file(path: impl AsRef<Path>) -> io::Result<CheckOutput> {
    let path = path.as_ref();
    let source = fs::read_to_string(path)?;
    let parsed = aura_parser::parse_program(&source);

    let mut diagnostics = parsed.errors;
    if diagnostics.is_empty() {
        if let Ok(graph) = modules::build_module_graph(&[path]) {
            if let Some(module) = graph.modules.iter().find(|m| m.path == path) {
                diagnostics.extend(resolve::resolve_module(module));
            }
        }
    }

    Ok(CheckOutput {
        source,
        diagnostics,
    })
}
