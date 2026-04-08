use std::fs;
use std::io;
use std::path::Path;

use aura_ast::Program;
use aura_diagnostics::Diagnostic;

pub mod dump_hir;
pub mod modules;
pub mod resolve;

use aura_mir::{lower_program, MirProgram};
use aura_typeck::TypedProgram;

#[derive(Clone, Debug)]
pub struct CheckOutput {
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
    pub typed_program: Option<TypedProgram>,
    pub ast: Option<Program>,
    pub mir: Option<MirProgram>,
}

pub fn check_file(path: impl AsRef<Path>) -> io::Result<CheckOutput> {
    let path = path.as_ref();
    let source = fs::read_to_string(path)?;
    let mut mir = None;
    let mut diagnostics = Vec::new();
    let mut typed_program = None;
    let mut ast = None;

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
                let is_clean = diags.is_empty();
                diagnostics.extend(diags);

                if is_clean {
                    mir = Some(lower_program(&module.source, &module.ast, &typed));
                }

                // For now, capture the last one (usually the entry point in simple cases)
                // or we could match by path.
                typed_program = Some(typed);
                ast = Some(module.ast.clone());
            }
        }
    } else {
        // Fall back to parsing the entry file only if graph construction fails.
        let parsed = aura_parser::parse_program(&source);
        diagnostics.extend(parsed.errors.clone());
        if parsed.errors.is_empty() {
            let (diags, typed) = aura_typeck::typeck_program(&source, &parsed.value);
            let is_clean = diags.is_empty();
            diagnostics.extend(diags);

            if is_clean {
                mir = Some(lower_program(&source, &parsed.value, &typed));
            }

            typed_program = Some(typed);
            ast = Some(parsed.value);
        }
    }

    Ok(CheckOutput {
        source,
        diagnostics,
        typed_program,
        ast,
        mir,
    })
}
