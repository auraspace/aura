use std::fs;
use std::io;
use std::path::Path;

use aura_diagnostics::Diagnostic;

pub mod modules;
pub mod resolve;

use aura_typeck::TypedProgram;

#[derive(Clone, Debug)]
pub struct CheckOutput {
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
    pub typed_program: Option<TypedProgram>,
}

pub fn check_file(path: impl AsRef<Path>) -> io::Result<CheckOutput> {
    let path = path.as_ref();
    let source = fs::read_to_string(path)?;
    let mut diagnostics = Vec::new();
    let mut typed_program = None;

    // Build a module graph (entrypoint + reachable relative imports).
    // We report parsing/resolution diagnostics across the whole graph.
    if let Ok(graph) = modules::build_module_graph(&[path]) {
        diagnostics.extend(modules::diagnose_missing_import_targets(&graph));
        for module in &graph.modules {
            diagnostics.extend(module.parse_diagnostics.clone());
        }

        // Only run name resolution for modules that parsed without errors.
        for module in &graph.modules {
            if module.parse_diagnostics.is_empty() {
                diagnostics.extend(resolve::resolve_module(module));
                let (diags, typed) = aura_typeck::typeck_program(&module.source, &module.ast);
                diagnostics.extend(diags);
                // For now, capture the last one (usually the entry point in simple cases)
                // or we could match by path.
                typed_program = Some(typed);
            }
        }
    } else {
        // Fall back to parsing the entry file only if graph construction fails.
        let parsed = aura_parser::parse_program(&source);
        diagnostics.extend(parsed.errors.clone());
        if parsed.errors.is_empty() {
            let (diags, typed) = aura_typeck::typeck_program(&source, &parsed.value);
            diagnostics.extend(diags);
            typed_program = Some(typed);
        }
    }

    Ok(CheckOutput {
        source,
        diagnostics,
        typed_program,
    })
}
