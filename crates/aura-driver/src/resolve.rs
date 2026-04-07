use std::collections::{HashMap, HashSet};

use aura_ast::{Expr, ImportClause, Stmt, TopLevel};
use aura_diagnostics::Diagnostic;
use aura_span::Span;

use crate::modules::{ident_text, build_symbol_table, Module};

pub fn resolve_module(module: &Module) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    diags.extend(check_module_duplicates(module));
    let symbols = build_symbol_table(module);
    let module_names: HashSet<String> = symbols.bindings.keys().cloned().collect();

    for item in &module.ast.items {
        match item {
            TopLevel::Function(func) => {
                let mut scopes = Vec::<HashSet<String>>::new();
                let mut function_scope = HashSet::<String>::new();
                for param in &func.params {
                    if let Some(name) = ident_text(&module.source, &param.name) {
                        if !function_scope.insert(name.clone()) {
                            diags.push(Diagnostic::error(
                                param.name.span,
                                format!("duplicate binding `{name}`"),
                            ));
                        }
                    }
                }
                scopes.push(function_scope);
                resolve_block_like(
                    module,
                    &func.body.stmts,
                    &mut scopes,
                    &module_names,
                    &mut diags,
                );
            }
            TopLevel::Stmt(stmt) => {
                let mut scopes = Vec::<HashSet<String>>::new();
                resolve_stmt(module, stmt, &mut scopes, &module_names, &mut diags);
            }
            TopLevel::Import(_) => {}
        }
    }

    diags
}

fn resolve_block_like(
    module: &Module,
    stmts: &[Stmt],
    scopes: &mut Vec<HashSet<String>>,
    module_names: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    scopes.push(HashSet::new());
    for stmt in stmts {
        resolve_stmt(module, stmt, scopes, module_names, diags);
    }
    scopes.pop();
}

fn resolve_stmt(
    module: &Module,
    stmt: &Stmt,
    scopes: &mut Vec<HashSet<String>>,
    module_names: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Let(s) | Stmt::Const(s) => {
            if let Some(init) = &s.init {
                resolve_expr(module, init, scopes, module_names, diags);
            }
            if let Some(name) = ident_text(&module.source, &s.name) {
                declare_local(scopes, name, s.name.span, diags);
            }
        }
        Stmt::Return(s) => {
            if let Some(value) = &s.value {
                resolve_expr(module, value, scopes, module_names, diags);
            }
        }
        Stmt::Expr(s) => resolve_expr(module, &s.expr, scopes, module_names, diags),
        Stmt::Block(b) => resolve_block_like(module, &b.stmts, scopes, module_names, diags),
        Stmt::If(s) => {
            resolve_expr(module, &s.cond, scopes, module_names, diags);
            resolve_block_like(module, &s.then_block.stmts, scopes, module_names, diags);
            if let Some(else_block) = &s.else_block {
                resolve_block_like(module, &else_block.stmts, scopes, module_names, diags);
            }
        }
        Stmt::While(s) => {
            resolve_expr(module, &s.cond, scopes, module_names, diags);
            resolve_block_like(module, &s.body.stmts, scopes, module_names, diags);
        }
        Stmt::Empty(_) => {}
    }
}

fn resolve_expr(
    module: &Module,
    expr: &Expr,
    scopes: &mut Vec<HashSet<String>>,
    module_names: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Ident(ident) => {
            let Some(name) = ident_text(&module.source, ident) else { return };
            if is_in_scopes(scopes, &name) || module_names.contains(&name) {
                return;
            }
            diags.push(Diagnostic::error(
                ident.span,
                format!("unknown identifier `{name}`"),
            ));
        }
        Expr::IntLit(_) | Expr::FloatLit(_) | Expr::StringLit(_) | Expr::BoolLit(_, _) => {}
        Expr::Unary { expr, .. } => resolve_expr(module, expr, scopes, module_names, diags),
        Expr::Binary { left, right, .. } => {
            resolve_expr(module, left, scopes, module_names, diags);
            resolve_expr(module, right, scopes, module_names, diags);
        }
        Expr::Assign { target, value, .. } => {
            resolve_expr(module, target, scopes, module_names, diags);
            resolve_expr(module, value, scopes, module_names, diags);
        }
        Expr::Call { callee, args, .. } => {
            resolve_expr(module, callee, scopes, module_names, diags);
            for arg in args {
                resolve_expr(module, arg, scopes, module_names, diags);
            }
        }
        Expr::Member { object, .. } => resolve_expr(module, object, scopes, module_names, diags),
        Expr::Paren { expr, .. } => resolve_expr(module, expr, scopes, module_names, diags),
    }
}

fn is_in_scopes(scopes: &[HashSet<String>], name: &str) -> bool {
    scopes.iter().rev().any(|s| s.contains(name))
}

fn declare_local(scopes: &mut Vec<HashSet<String>>, name: String, span: Span, diags: &mut Vec<Diagnostic>) {
    if scopes.is_empty() {
        scopes.push(HashSet::new());
    }
    let current = scopes.last_mut().unwrap();
    if !current.insert(name.clone()) {
        diags.push(Diagnostic::error(span, format!("duplicate binding `{name}`")));
    }
}

fn check_module_duplicates(module: &Module) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut seen = HashMap::<String, Span>::new();

    for item in &module.ast.items {
        match item {
            TopLevel::Import(import) => match &import.clause {
                ImportClause::Named(names) => {
                    for name in names {
                        if let Some(text) = ident_text(&module.source, name) {
                            check_dup(&mut seen, &mut diags, text, name.span);
                        }
                    }
                }
                ImportClause::Default(name) => {
                    if let Some(text) = ident_text(&module.source, name) {
                        check_dup(&mut seen, &mut diags, text, name.span);
                    }
                }
            },
            TopLevel::Function(func) => {
                if let Some(text) = ident_text(&module.source, &func.name) {
                    check_dup(&mut seen, &mut diags, text, func.name.span);
                }
            }
            TopLevel::Stmt(stmt) => match stmt {
                Stmt::Let(s) | Stmt::Const(s) => {
                    if let Some(text) = ident_text(&module.source, &s.name) {
                        check_dup(&mut seen, &mut diags, text, s.name.span);
                    }
                }
                _ => {}
            },
        }
    }

    diags
}

fn check_dup(seen: &mut HashMap<String, Span>, diags: &mut Vec<Diagnostic>, name: String, span: Span) {
    if seen.contains_key(&name) {
        diags.push(Diagnostic::error(span, format!("duplicate binding `{name}`")));
    } else {
        seen.insert(name, span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn unique_tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("aura-driver-resolve-{name}-{nonce}"));
        p
    }

    #[test]
    fn reports_unknown_identifier_in_function_body() {
        let dir = unique_tmp_dir("unknown-ident");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        fs::write(
            &main,
            r#"
function main(): i32 {
  return missing;
}
"#,
        )
        .unwrap();

        let graph = crate::modules::build_module_graph(&[&main]).unwrap();
        let module = graph.modules.iter().find(|m| m.path == main).unwrap();
        let diags = resolve_module(module);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown identifier"));
    }

    #[test]
    fn accepts_local_let_and_params() {
        let dir = unique_tmp_dir("locals");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        fs::write(
            &main,
            r#"
function add(x: i32): i32 {
  let y: i32 = x;
  return y;
}
"#,
        )
        .unwrap();

        let graph = crate::modules::build_module_graph(&[&main]).unwrap();
        let module = graph.modules.iter().find(|m| m.path == main).unwrap();
        let diags = resolve_module(module);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn reports_duplicate_binding_in_same_scope() {
        let dir = unique_tmp_dir("dup-local");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        fs::write(
            &main,
            r#"
function f(): i32 {
  let x: i32 = 1;
  let x: i32 = 2;
  return x;
}
"#,
        )
        .unwrap();

        let graph = crate::modules::build_module_graph(&[&main]).unwrap();
        let module = graph.modules.iter().find(|m| m.path == main).unwrap();
        let diags = resolve_module(module);
        assert!(diags.iter().any(|d| d.message.contains("duplicate binding")));
    }

    #[test]
    fn reports_duplicate_module_binding() {
        let dir = unique_tmp_dir("dup-module");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        fs::write(
            &main,
            r#"
function foo(): i32 { return 0; }
function foo(): i32 { return 1; }
"#,
        )
        .unwrap();

        let graph = crate::modules::build_module_graph(&[&main]).unwrap();
        let module = graph.modules.iter().find(|m| m.path == main).unwrap();
        let diags = resolve_module(module);
        assert!(diags.iter().any(|d| d.message.contains("duplicate binding")));
    }
}
