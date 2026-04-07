use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use aura_ast::{ImportClause, TopLevel};
use aura_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleGraph {
    pub modules: Vec<Module>,
    pub edges: Vec<ImportEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub path: PathBuf,
    pub source: String,
    pub imports: Vec<ImportEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportEdge {
    pub from: PathBuf,
    pub clause: ImportClauseKind,
    /// Raw specifier text (decoded from the string literal; does not include quotes).
    pub specifier: String,
    /// Span of the original string literal token (includes surrounding quotes).
    pub specifier_span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportClauseKind {
    Named { count: usize },
    Default,
}

pub fn build_module_graph(entrypoints: &[impl AsRef<Path>]) -> io::Result<ModuleGraph> {
    let mut modules = Vec::new();
    let mut edges = Vec::new();

    for entry in entrypoints {
        let path = entry.as_ref().to_path_buf();
        let source = fs::read_to_string(&path)?;
        let parsed = aura_parser::parse_program(&source);

        let mut imports = Vec::new();
        for item in parsed.value.items {
            let TopLevel::Import(import) = item else { continue };
            let Some(specifier) = decode_string_literal(&source, import.from_path) else {
                continue;
            };

            let clause = match import.clause {
                ImportClause::Named(names) => ImportClauseKind::Named { count: names.len() },
                ImportClause::Default(_) => ImportClauseKind::Default,
            };

            let edge = ImportEdge {
                from: path.clone(),
                clause,
                specifier,
                specifier_span: import.from_path,
            };
            edges.push(edge.clone());
            imports.push(edge);
        }

        modules.push(Module { path, source, imports });
    }

    Ok(ModuleGraph { modules, edges })
}

fn decode_string_literal(source: &str, span: Span) -> Option<String> {
    let start = span.start.raw() as usize;
    let end = span.end.raw() as usize;
    let text = source.get(start..end)?;
    let text = text.strip_prefix('"')?.strip_suffix('"')?;
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("aura-driver-{name}-{nonce}"));
        p
    }

    #[test]
    fn module_graph_collects_import_edges_from_entrypoints() {
        let dir = unique_tmp_dir("module-graph");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        fs::write(
            &main,
            r#"
import { Foo, bar } from "./foo"
import Baz from "./baz";

function main(): i32 { return 0; }
"#,
        )
        .unwrap();

        let graph = build_module_graph(&[&main]).unwrap();
        assert_eq!(graph.modules.len(), 1);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].specifier, "./foo");
        assert_eq!(graph.edges[1].specifier, "./baz");
        assert!(matches!(
            graph.edges[0].clause,
            ImportClauseKind::Named { count: 2 }
        ));
        assert!(matches!(graph.edges[1].clause, ImportClauseKind::Default));
    }
}

