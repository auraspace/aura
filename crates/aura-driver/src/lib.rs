use std::fs;
use std::io;
use std::path::Path;

use aura_ast::Program;
use aura_diagnostics::Diagnostic;
use aura_typeck::MethodSig;

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

        let entry_module = graph.modules.iter().find(|module| module.path == path);
        let entry_path = entry_module.map(|module| module.path.clone());
        let mut exported_functions_by_module = std::collections::HashMap::<
            std::path::PathBuf,
            std::collections::HashMap<String, MethodSig>,
        >::new();
        let mut combined_mir: Option<MirProgram> = None;

        for module in &graph.modules {
            if entry_path.as_ref() == Some(&module.path) {
                continue;
            }
            if module.parse_diagnostics.is_empty() {
                diagnostics.extend(resolve::resolve_module(module));
                let (diags, typed) = aura_typeck::typeck_program(&module.source, &module.ast);
                let is_clean = diags.is_empty();
                diagnostics.extend(diags);
                exported_functions_by_module.insert(module.path.clone(), typed.functions.clone());
                if is_clean {
                    let module_mir = lower_program(&module.source, &module.ast, &typed);
                    merge_mir_program(&mut combined_mir, module_mir);
                }
            }
        }

        if let Some(module) = entry_module {
            let imported_functions =
                imported_functions_for_module(module, &graph, &exported_functions_by_module);
            if module.parse_diagnostics.is_empty() {
                diagnostics.extend(resolve::resolve_module(module));
                let (diags, typed) = aura_typeck::typeck_program_with_imports(
                    &module.source,
                    &module.ast,
                    &imported_functions,
                );
                let is_clean = diags.is_empty();
                diagnostics.extend(diags);

                if is_clean {
                    let module_mir = lower_program(&module.source, &module.ast, &typed);
                    merge_mir_program(&mut combined_mir, module_mir);
                }

                typed_program = Some(typed);
                ast = Some(module.ast.clone());
            }
        }

        mir = combined_mir;
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

fn merge_mir_program(dst: &mut Option<MirProgram>, src: MirProgram) {
    match dst {
        Some(existing) => {
            existing.functions.extend(src.functions);
            for (name, class) in src.classes {
                existing.classes.entry(name).or_insert(class);
            }
            for (name, interface) in src.interfaces {
                existing.interfaces.entry(name).or_insert(interface);
            }
            for slot in src.method_slots {
                if !existing.method_slots.contains(&slot) {
                    existing.method_slots.push(slot);
                }
            }
        }
        None => {
            *dst = Some(src);
        }
    }
}

fn imported_functions_for_module(
    module: &modules::Module,
    graph: &modules::ModuleGraph,
    exported_functions_by_module: &std::collections::HashMap<
        std::path::PathBuf,
        std::collections::HashMap<String, MethodSig>,
    >,
) -> std::collections::HashMap<String, MethodSig> {
    let mut imported_functions = std::collections::HashMap::new();

    for item in &module.ast.items {
        let aura_ast::TopLevel::Import(import) = item else {
            continue;
        };

        let Some(edge) = graph
            .edges
            .iter()
            .find(|edge| edge.from == module.path && edge.specifier_span == import.from_path)
        else {
            continue;
        };
        let Some(target_path) = edge.resolved_to.as_ref() else {
            continue;
        };
        let Some(target_exports) = exported_functions_by_module.get(target_path) else {
            continue;
        };

        if let aura_ast::ImportClause::Named(names) = &import.clause {
            for name in names {
                if let Some(name_text) = modules::ident_text(&module.source, name) {
                    if let Some(sig) = target_exports.get(&name_text) {
                        imported_functions
                            .entry(name_text)
                            .or_insert_with(|| sig.clone());
                    }
                }
            }
        }
    }

    imported_functions
}
