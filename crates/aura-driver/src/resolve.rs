use std::collections::{HashMap, HashSet};

use aura_ast::{ExportedDecl, Expr, ImportClause, Stmt, TopLevel};
use aura_diagnostics::Diagnostic;
use aura_span::Span;

use crate::modules::{build_symbol_table, ident_text, Module, SymbolKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberAccess {
    pub object_span: Span,
    pub field: String,
    pub span: Span,
}

pub fn resolve_module(module: &Module) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    diags.extend(check_module_duplicates(module));
    let symbols = build_symbol_table(module);
    let module_names: HashSet<String> = symbols
        .bindings
        .iter()
        .filter_map(|(name, symbol)| match symbol.kind {
            SymbolKind::Function | SymbolKind::Let | SymbolKind::Const | SymbolKind::Import => {
                Some(name.clone())
            }
            SymbolKind::Class | SymbolKind::Interface => None,
        })
        .collect();

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
                    false,
                    &mut diags,
                );
            }
            TopLevel::Export(export) => match export.item.as_ref() {
                Some(ExportedDecl::Function(func)) => {
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
                        false,
                        &mut diags,
                    );
                }
                Some(ExportedDecl::Class(class_decl)) => {
                    for method in &class_decl.methods {
                        let mut scopes = Vec::<HashSet<String>>::new();
                        let mut method_scope = HashSet::<String>::new();
                        for param in &method.params {
                            if let Some(name) = ident_text(&module.source, &param.name) {
                                if !method_scope.insert(name.clone()) {
                                    diags.push(Diagnostic::error(
                                        param.name.span,
                                        format!("duplicate binding `{name}`"),
                                    ));
                                }
                            }
                        }
                        scopes.push(method_scope);
                        resolve_block_like(
                            module,
                            &method.body.stmts,
                            &mut scopes,
                            &module_names,
                            true,
                            &mut diags,
                        );
                    }
                }
                Some(ExportedDecl::Interface(_)) | None => {}
            },
            TopLevel::Class(class_decl) => {
                for method in &class_decl.methods {
                    let mut scopes = Vec::<HashSet<String>>::new();
                    let mut method_scope = HashSet::<String>::new();
                    for param in &method.params {
                        if let Some(name) = ident_text(&module.source, &param.name) {
                            if !method_scope.insert(name.clone()) {
                                diags.push(Diagnostic::error(
                                    param.name.span,
                                    format!("duplicate binding `{name}`"),
                                ));
                            }
                        }
                    }
                    scopes.push(method_scope);
                    resolve_block_like(
                        module,
                        &method.body.stmts,
                        &mut scopes,
                        &module_names,
                        true,
                        &mut diags,
                    );
                }
            }
            TopLevel::Stmt(stmt) => {
                let mut scopes = Vec::<HashSet<String>>::new();
                resolve_stmt(module, stmt, &mut scopes, &module_names, false, &mut diags);
            }
            TopLevel::Import(_) | TopLevel::Interface(_) => {}
        }
    }

    diags
}

pub fn collect_member_accesses(module: &Module) -> Vec<MemberAccess> {
    let mut out = Vec::new();
    for item in &module.ast.items {
        match item {
            TopLevel::Function(func) => {
                for stmt in &func.body.stmts {
                    collect_member_accesses_in_stmt(module, stmt, &mut out);
                }
            }
            TopLevel::Export(export) => match export.item.as_ref() {
                Some(ExportedDecl::Function(func)) => {
                    for stmt in &func.body.stmts {
                        collect_member_accesses_in_stmt(module, stmt, &mut out);
                    }
                }
                Some(ExportedDecl::Class(class_decl)) => {
                    for method in &class_decl.methods {
                        for stmt in &method.body.stmts {
                            collect_member_accesses_in_stmt(module, stmt, &mut out);
                        }
                    }
                }
                Some(ExportedDecl::Interface(_)) | None => {}
            },
            TopLevel::Class(class_decl) => {
                for method in &class_decl.methods {
                    for stmt in &method.body.stmts {
                        collect_member_accesses_in_stmt(module, stmt, &mut out);
                    }
                }
            }
            TopLevel::Stmt(stmt) => collect_member_accesses_in_stmt(module, stmt, &mut out),
            TopLevel::Import(_) | TopLevel::Interface(_) => {}
        }
    }
    out
}

fn collect_member_accesses_in_stmt(module: &Module, stmt: &Stmt, out: &mut Vec<MemberAccess>) {
    match stmt {
        Stmt::Let(s) | Stmt::Const(s) => {
            if let Some(init) = &s.init {
                collect_member_accesses_in_expr(module, init, out);
            }
        }
        Stmt::Return(s) => {
            if let Some(value) = &s.value {
                collect_member_accesses_in_expr(module, value, out);
            }
        }
        Stmt::Throw(s) => collect_member_accesses_in_expr(module, &s.value, out),
        Stmt::Expr(s) => collect_member_accesses_in_expr(module, &s.expr, out),
        Stmt::Block(b) => {
            for stmt in &b.stmts {
                collect_member_accesses_in_stmt(module, stmt, out);
            }
        }
        Stmt::If(s) => {
            collect_member_accesses_in_expr(module, &s.cond, out);
            for stmt in &s.then_block.stmts {
                collect_member_accesses_in_stmt(module, stmt, out);
            }
            if let Some(else_block) = &s.else_block {
                for stmt in &else_block.stmts {
                    collect_member_accesses_in_stmt(module, stmt, out);
                }
            }
        }
        Stmt::While(s) => {
            collect_member_accesses_in_expr(module, &s.cond, out);
            for stmt in &s.body.stmts {
                collect_member_accesses_in_stmt(module, stmt, out);
            }
        }
        Stmt::Try(s) => {
            for stmt in &s.try_block.stmts {
                collect_member_accesses_in_stmt(module, stmt, out);
            }
            if let Some(catch) = &s.catch {
                for stmt in &catch.block.stmts {
                    collect_member_accesses_in_stmt(module, stmt, out);
                }
            }
            if let Some(finally_block) = &s.finally_block {
                for stmt in &finally_block.stmts {
                    collect_member_accesses_in_stmt(module, stmt, out);
                }
            }
        }
        Stmt::Empty(_) => {}
    }
}

fn collect_member_accesses_in_expr(module: &Module, expr: &Expr, out: &mut Vec<MemberAccess>) {
    match expr {
        Expr::This(_) => {}
        Expr::Unary { expr, .. } => collect_member_accesses_in_expr(module, expr, out),
        Expr::Binary { left, right, .. } => {
            collect_member_accesses_in_expr(module, left, out);
            collect_member_accesses_in_expr(module, right, out);
        }
        Expr::Assign { target, value, .. } => {
            collect_member_accesses_in_expr(module, target, out);
            collect_member_accesses_in_expr(module, value, out);
        }
        Expr::Call { callee, args, .. } => {
            collect_member_accesses_in_expr(module, callee, out);
            for arg in args {
                collect_member_accesses_in_expr(module, arg, out);
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                collect_member_accesses_in_expr(module, arg, out);
            }
        }
        Expr::Member {
            object,
            field,
            span,
        } => {
            collect_member_accesses_in_expr(module, object, out);
            let field_name = ident_text(&module.source, field).unwrap_or_default();
            out.push(MemberAccess {
                object_span: span_of_expr(object),
                field: field_name,
                span: *span,
            });
        }
        Expr::Paren { expr, .. } => collect_member_accesses_in_expr(module, expr, out),
        Expr::Ident(_)
        | Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_, _) => {}
    }
}

fn span_of_expr(expr: &Expr) -> Span {
    match expr {
        Expr::Ident(i) => i.span,
        Expr::This(s) => *s,
        Expr::IntLit(s) => *s,
        Expr::FloatLit(s) => *s,
        Expr::StringLit(s) => *s,
        Expr::BoolLit(_, s) => *s,
        Expr::Unary { span, .. } => *span,
        Expr::Binary { span, .. } => *span,
        Expr::Assign { span, .. } => *span,
        Expr::Call { span, .. } => *span,
        Expr::New { span, .. } => *span,
        Expr::Member { span, .. } => *span,
        Expr::Paren { span, .. } => *span,
    }
}

fn resolve_block_like(
    module: &Module,
    stmts: &[Stmt],
    scopes: &mut Vec<HashSet<String>>,
    module_names: &HashSet<String>,
    this_allowed: bool,
    diags: &mut Vec<Diagnostic>,
) {
    scopes.push(HashSet::new());
    for stmt in stmts {
        resolve_stmt(module, stmt, scopes, module_names, this_allowed, diags);
    }
    scopes.pop();
}

fn resolve_stmt(
    module: &Module,
    stmt: &Stmt,
    scopes: &mut Vec<HashSet<String>>,
    module_names: &HashSet<String>,
    this_allowed: bool,
    diags: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Let(s) | Stmt::Const(s) => {
            if let Some(init) = &s.init {
                resolve_expr(module, init, scopes, module_names, this_allowed, diags);
            }
            if let Some(name) = ident_text(&module.source, &s.name) {
                declare_local(scopes, name, s.name.span, diags);
            }
        }
        Stmt::Return(s) => {
            if let Some(value) = &s.value {
                resolve_expr(module, value, scopes, module_names, this_allowed, diags);
            }
        }
        Stmt::Throw(s) => {
            resolve_expr(module, &s.value, scopes, module_names, this_allowed, diags);
        }
        Stmt::Expr(s) => resolve_expr(module, &s.expr, scopes, module_names, this_allowed, diags),
        Stmt::Block(b) => {
            resolve_block_like(module, &b.stmts, scopes, module_names, this_allowed, diags)
        }
        Stmt::If(s) => {
            resolve_expr(module, &s.cond, scopes, module_names, this_allowed, diags);
            resolve_block_like(
                module,
                &s.then_block.stmts,
                scopes,
                module_names,
                this_allowed,
                diags,
            );
            if let Some(else_block) = &s.else_block {
                resolve_block_like(
                    module,
                    &else_block.stmts,
                    scopes,
                    module_names,
                    this_allowed,
                    diags,
                );
            }
        }
        Stmt::While(s) => {
            resolve_expr(module, &s.cond, scopes, module_names, this_allowed, diags);
            resolve_block_like(
                module,
                &s.body.stmts,
                scopes,
                module_names,
                this_allowed,
                diags,
            );
        }
        Stmt::Try(s) => {
            resolve_block_like(
                module,
                &s.try_block.stmts,
                scopes,
                module_names,
                this_allowed,
                diags,
            );
            if let Some(catch) = &s.catch {
                scopes.push(HashSet::new());
                if let Some(name) = ident_text(&module.source, &catch.binding) {
                    declare_local(scopes, name, catch.binding.span, diags);
                }
                resolve_block_like(
                    module,
                    &catch.block.stmts,
                    scopes,
                    module_names,
                    this_allowed,
                    diags,
                );
                scopes.pop();
            }
            if let Some(finally_block) = &s.finally_block {
                resolve_block_like(
                    module,
                    &finally_block.stmts,
                    scopes,
                    module_names,
                    this_allowed,
                    diags,
                );
            }
        }
        Stmt::Empty(_) => {}
    }
}

fn resolve_expr(
    module: &Module,
    expr: &Expr,
    scopes: &mut Vec<HashSet<String>>,
    module_names: &HashSet<String>,
    this_allowed: bool,
    diags: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::This(span) => {
            if !this_allowed {
                diags.push(Diagnostic::error(
                    *span,
                    "invalid use of `this` outside of a class method".to_string(),
                ));
            }
        }
        Expr::Ident(ident) => {
            let Some(name) = ident_text(&module.source, ident) else {
                return;
            };
            if is_in_scopes(scopes, &name) || module_names.contains(&name) || name == "println" {
                return;
            }
            diags.push(Diagnostic::error(
                ident.span,
                format!("unknown identifier `{name}`"),
            ));
        }
        Expr::IntLit(_) | Expr::FloatLit(_) | Expr::StringLit(_) | Expr::BoolLit(_, _) => {}
        Expr::Unary { expr, .. } => {
            resolve_expr(module, expr, scopes, module_names, this_allowed, diags)
        }
        Expr::Binary { left, right, .. } => {
            resolve_expr(module, left, scopes, module_names, this_allowed, diags);
            resolve_expr(module, right, scopes, module_names, this_allowed, diags);
        }
        Expr::Assign { target, value, .. } => {
            resolve_expr(module, target, scopes, module_names, this_allowed, diags);
            resolve_expr(module, value, scopes, module_names, this_allowed, diags);
        }
        Expr::Call { callee, args, .. } => {
            resolve_expr(module, callee, scopes, module_names, this_allowed, diags);
            for arg in args {
                resolve_expr(module, arg, scopes, module_names, this_allowed, diags);
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                resolve_expr(module, arg, scopes, module_names, this_allowed, diags);
            }
        }
        Expr::Member { object, .. } => {
            resolve_expr(module, object, scopes, module_names, this_allowed, diags)
        }
        Expr::Paren { expr, .. } => {
            resolve_expr(module, expr, scopes, module_names, this_allowed, diags)
        }
    }
}

fn is_in_scopes(scopes: &[HashSet<String>], name: &str) -> bool {
    scopes.iter().rev().any(|s| s.contains(name))
}

fn declare_local(
    scopes: &mut Vec<HashSet<String>>,
    name: String,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    if scopes.is_empty() {
        scopes.push(HashSet::new());
    }
    let current = scopes.last_mut().unwrap();
    if !current.insert(name.clone()) {
        diags.push(Diagnostic::error(
            span,
            format!("duplicate binding `{name}`"),
        ));
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
            TopLevel::Class(class_decl) => {
                if let Some(text) = ident_text(&module.source, &class_decl.name) {
                    check_dup(&mut seen, &mut diags, text, class_decl.name.span);
                }
            }
            TopLevel::Stmt(stmt) => match stmt {
                Stmt::Let(s) | Stmt::Const(s) => {
                    if let Some(text) = ident_text(&module.source, &s.name) {
                        check_dup(&mut seen, &mut diags, text, s.name.span);
                    }
                }
                Stmt::Try(s) => {
                    if let Some(catch) = &s.catch {
                        if let Some(text) = ident_text(&module.source, &catch.binding) {
                            check_dup(&mut seen, &mut diags, text, catch.binding.span);
                        }
                    }
                }
                _ => {}
            },
            TopLevel::Interface(iface_decl) => {
                if let Some(text) = ident_text(&module.source, &iface_decl.name) {
                    check_dup(&mut seen, &mut diags, text, iface_decl.name.span);
                }
            }
            TopLevel::Export(export) => match export.item.as_ref() {
                Some(ExportedDecl::Function(func)) => {
                    if let Some(text) = ident_text(&module.source, &func.name) {
                        check_dup(&mut seen, &mut diags, text, func.name.span);
                    }
                }
                Some(ExportedDecl::Class(class_decl)) => {
                    if let Some(text) = ident_text(&module.source, &class_decl.name) {
                        check_dup(&mut seen, &mut diags, text, class_decl.name.span);
                    }
                }
                Some(ExportedDecl::Interface(iface_decl)) => {
                    if let Some(text) = ident_text(&module.source, &iface_decl.name) {
                        check_dup(&mut seen, &mut diags, text, iface_decl.name.span);
                    }
                }
                None => {}
            },
        }
    }

    diags
}

fn check_dup(
    seen: &mut HashMap<String, Span>,
    diags: &mut Vec<Diagnostic>,
    name: String,
    span: Span,
) {
    if seen.contains_key(&name) {
        diags.push(Diagnostic::error(
            span,
            format!("duplicate binding `{name}`"),
        ));
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
        assert!(diags
            .iter()
            .any(|d| d.message.contains("duplicate binding")));
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
        assert!(diags
            .iter()
            .any(|d| d.message.contains("duplicate binding")));
    }

    #[test]
    fn collects_member_accesses() {
        let dir = unique_tmp_dir("members");
        fs::create_dir_all(&dir).unwrap();

        let main = dir.join("main.aura");
        fs::write(
            &main,
            r#"
function f(): i32 {
  return a.b;
}
"#,
        )
        .unwrap();

        let graph = crate::modules::build_module_graph(&[&main]).unwrap();
        let module = graph.modules.iter().find(|m| m.path == main).unwrap();
        let members = collect_member_accesses(module);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].field, "b");
    }
}
