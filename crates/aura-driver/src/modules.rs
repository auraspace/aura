use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::collections::{HashSet, VecDeque};

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
    /// Resolved target file path (relative imports only; extension inferred per MVP rules).
    pub resolved_to: Option<PathBuf>,
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

    let mut seen = HashSet::<PathBuf>::new();
    let mut worklist = VecDeque::<PathBuf>::new();

    for entry in entrypoints {
        let path = entry.as_ref().to_path_buf();
        if seen.insert(path.clone()) {
            worklist.push_back(path);
        }
    }

    while let Some(path) = worklist.pop_front() {
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

            let resolved_to = resolve_import_path(&path, &specifier);
            if let Some(target) = &resolved_to {
                if seen.insert(target.clone()) {
                    worklist.push_back(target.clone());
                }
            }

            let edge = ImportEdge {
                from: path.clone(),
                clause,
                specifier,
                resolved_to,
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

fn resolve_import_path(from_file: &Path, specifier: &str) -> Option<PathBuf> {
    if !(specifier.starts_with("./") || specifier.starts_with("../")) {
        return None;
    }
    let base = from_file.parent()?;
    let joined = base.join(specifier);

    // If an extension is already provided, honor it.
    if joined.extension().is_some() {
        return joined.is_file().then_some(joined);
    }

    // MVP: omit file extension. Try `.aura` first, then `.ar`.
    let aura = joined.with_extension("aura");
    if aura.is_file() {
        return Some(aura);
    }
    let ar = joined.with_extension("ar");
    if ar.is_file() {
        return Some(ar);
    }

    None
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
        let foo = dir.join("foo.aura");
        let baz = dir.join("baz.ar");
        fs::write(
            &main,
            r#"
import { Foo, bar } from "./foo"
import Baz from "./baz";

function main(): i32 { return 0; }
"#,
        )
        .unwrap();
        fs::write(&foo, "function foo(): i32 { return 1; }").unwrap();
        fs::write(&baz, "function baz(): i32 { return 2; }").unwrap();

        let graph = build_module_graph(&[&main]).unwrap();
        assert_eq!(graph.modules.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].specifier, "./foo");
        assert_eq!(graph.edges[1].specifier, "./baz");
        assert_eq!(graph.edges[0].resolved_to.as_deref(), Some(foo.as_path()));
        assert_eq!(graph.edges[1].resolved_to.as_deref(), Some(baz.as_path()));
        assert!(matches!(
            graph.edges[0].clause,
            ImportClauseKind::Named { count: 2 }
        ));
        assert!(matches!(graph.edges[1].clause, ImportClauseKind::Default));
    }

    #[test]
    fn resolve_import_prefers_aura_over_ar() {
        let dir = unique_tmp_dir("resolve-prefer-aura");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        let foo_aura = dir.join("foo.aura");
        let foo_ar = dir.join("foo.ar");

        fs::write(
            &main,
            r#"
import Foo from "./foo";
function main(): i32 { return 0; }
"#,
        )
        .unwrap();
        fs::write(&foo_aura, "function foo(): i32 { return 1; }").unwrap();
        fs::write(&foo_ar, "function foo(): i32 { return 2; }").unwrap();

        let graph = build_module_graph(&[&main]).unwrap();
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(
            graph.edges[0].resolved_to.as_deref(),
            Some(foo_aura.as_path())
        );
    }
}
