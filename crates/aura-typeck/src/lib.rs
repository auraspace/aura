use aura_ast::{Program, Stmt, TopLevel, TypeRef};
use aura_diagnostics::Diagnostic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Void,
}

impl Ty {
    pub const fn as_str(self) -> &'static str {
        match self {
            Ty::I32 => "i32",
            Ty::I64 => "i64",
            Ty::F32 => "f32",
            Ty::F64 => "f64",
            Ty::Bool => "bool",
            Ty::String => "string",
            Ty::Void => "void",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypePosition {
    Value,
    Return,
}

pub fn typeck_program(source: &str, program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for item in &program.items {
        match item {
            TopLevel::Import(_) => {}
            TopLevel::Function(func) => {
                for param in &func.params {
                    check_type_ref(source, &param.ty, TypePosition::Value, &mut diags);
                }
                if let Some(ret) = &func.return_type {
                    check_type_ref(source, ret, TypePosition::Return, &mut diags);
                }
                // No expression typing yet (Phase 3 TODOs).
            }
            TopLevel::Stmt(stmt) => {
                check_stmt_types(source, stmt, &mut diags);
            }
        }
    }

    diags
}

fn check_stmt_types(source: &str, stmt: &Stmt, diags: &mut Vec<Diagnostic>) {
    match stmt {
        Stmt::Let(s) | Stmt::Const(s) => {
            if let Some(ty) = &s.ty {
                check_type_ref(source, ty, TypePosition::Value, diags);
            }
        }
        Stmt::Block(b) => {
            for stmt in &b.stmts {
                check_stmt_types(source, stmt, diags);
            }
        }
        Stmt::If(s) => {
            for stmt in &s.then_block.stmts {
                check_stmt_types(source, stmt, diags);
            }
            if let Some(else_block) = &s.else_block {
                for stmt in &else_block.stmts {
                    check_stmt_types(source, stmt, diags);
                }
            }
        }
        Stmt::While(s) => {
            for stmt in &s.body.stmts {
                check_stmt_types(source, stmt, diags);
            }
        }
        Stmt::Return(_) | Stmt::Expr(_) | Stmt::Empty(_) => {}
    }
}

fn check_type_ref(source: &str, ty: &TypeRef, pos: TypePosition, diags: &mut Vec<Diagnostic>) {
    let Some(name) = ident_text(source, ty) else {
        diags.push(Diagnostic::error(ty.span, "invalid type reference"));
        return;
    };

    let Some(builtin) = parse_builtin_ty(name) else {
        diags.push(
            Diagnostic::error(ty.span, format!("unknown type `{name}`")).with_help(
                "built-in types: i32, i64, f32, f64, bool, string, void",
            ),
        );
        return;
    };

    if builtin == Ty::Void && pos != TypePosition::Return {
        diags.push(
            Diagnostic::error(ty.span, "type `void` is only valid as a function return type")
                .with_help("remove the annotation or use a non-void type"),
        );
    }
}

fn parse_builtin_ty(name: &str) -> Option<Ty> {
    match name {
        "i32" => Some(Ty::I32),
        "i64" => Some(Ty::I64),
        "f32" => Some(Ty::F32),
        "f64" => Some(Ty::F64),
        "bool" => Some(Ty::Bool),
        "string" => Some(Ty::String),
        "void" => Some(Ty::Void),
        _ => None,
    }
}

fn ident_text<'a>(source: &'a str, ty: &TypeRef) -> Option<&'a str> {
    let start = ty.name.span.start.raw() as usize;
    let end = ty.name.span.end.raw() as usize;
    source.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_builtin_types() {
        let src = r#"
function f(a: i32, b: string): void { return; }
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn reports_unknown_type() {
        let src = r#"
function f(a: Foo): i32 { return 0; }
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown type"));
    }

    #[test]
    fn rejects_void_outside_return_position() {
        let src = r#"
function f(a: void): i32 { return 0; }
let x: void = 0;
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 2, "{diags:#?}");
        assert!(diags
            .iter()
            .all(|d| d.message.contains("only valid as a function return type")));
    }
}

