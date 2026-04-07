use std::collections::HashMap;

use aura_ast::{Expr, Ident, LetStmt, Program, Stmt, TopLevel, TypeRef};
use aura_diagnostics::Diagnostic;
use aura_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Unknown,
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
            Ty::Unknown => "<unknown>",
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

    // Pass 1: predeclare all top-level `let/const` so later type checking can resolve
    // global names regardless of declaration order (matching resolver behavior).
    let mut globals = HashMap::<String, VarInfo>::new();
    for item in &program.items {
        let TopLevel::Stmt(Stmt::Let(s) | Stmt::Const(s)) = item else {
            continue;
        };
        let Some(name) = ident_text(source, &s.name) else { continue };
        globals.entry(name.to_string()).or_insert_with(|| VarInfo {
            // Defer annotation checking to Pass 2 to avoid duplicate diagnostics.
            ty: Ty::Unknown,
            mutable: matches!(item, TopLevel::Stmt(Stmt::Let(_))),
            decl_span: s.name.span,
        });
    }

    let mut global_env = Env::new(globals);

    // Pass 2: type-check all top-level statements (including global var initializers).
    for item in &program.items {
        let TopLevel::Stmt(stmt) = item else { continue };
        typeck_stmt(source, &mut global_env, stmt, &mut diags);
    }

    // Pass 3: type-check function signatures and bodies.
    for item in &program.items {
        let TopLevel::Function(func) = item else { continue };
        if let Some(ret) = &func.return_type {
            let _ = ty_from_type_ref(source, ret, TypePosition::Return, &mut diags);
        }

        let mut env = global_env.child();
        for param in &func.params {
            let Some(name) = ident_text(source, &param.name) else { continue };
            let ty = ty_from_type_ref(source, &param.ty, TypePosition::Value, &mut diags);
            env.declare(
                name.to_string(),
                VarInfo {
                    ty,
                    mutable: true,
                    decl_span: param.name.span,
                },
            );
        }

        // No return-path checking yet (Phase 3 TODOs).
        typeck_block(source, &mut env, &func.body, &mut diags);
    }

    diags
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VarInfo {
    ty: Ty,
    mutable: bool,
    decl_span: Span,
}

#[derive(Clone, Debug)]
struct Env {
    scopes: Vec<HashMap<String, VarInfo>>,
}

impl Env {
    fn new(globals: HashMap<String, VarInfo>) -> Self {
        Self {
            scopes: vec![globals],
        }
    }

    fn child(&self) -> Self {
        Self {
            scopes: vec![self.scopes[0].clone()],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn declare(&mut self, name: String, info: VarInfo) {
        let current = self.scopes.last_mut().unwrap();
        current.entry(name).or_insert(info);
    }

    fn lookup(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v);
            }
        }
        None
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        None
    }
}

fn typeck_block(source: &str, env: &mut Env, block: &aura_ast::Block, diags: &mut Vec<Diagnostic>) {
    env.push_scope();
    for stmt in &block.stmts {
        typeck_stmt(source, env, stmt, diags);
    }
    env.pop_scope();
}

fn typeck_stmt(source: &str, env: &mut Env, stmt: &Stmt, diags: &mut Vec<Diagnostic>) {
    match stmt {
        Stmt::Let(s) => typeck_let_like(source, env, s, true, diags),
        Stmt::Const(s) => typeck_let_like(source, env, s, false, diags),
        Stmt::Return(s) => {
            if let Some(value) = &s.value {
                let _ = typeck_expr(source, env, value, diags);
            }
        }
        Stmt::Expr(s) => {
            let _ = typeck_expr(source, env, &s.expr, diags);
        }
        Stmt::Block(b) => typeck_block(source, env, b, diags),
        Stmt::If(s) => {
            let cond_ty = typeck_expr(source, env, &s.cond, diags);
            if cond_ty != Ty::Unknown && cond_ty != Ty::Bool {
                diags.push(
                    Diagnostic::error(span_of_expr(&s.cond), "condition must be of type `bool`")
                        .with_help(format!("got `{}`", cond_ty.as_str())),
                );
            }
            typeck_block(source, env, &s.then_block, diags);
            if let Some(else_block) = &s.else_block {
                typeck_block(source, env, else_block, diags);
            }
        }
        Stmt::While(s) => {
            let cond_ty = typeck_expr(source, env, &s.cond, diags);
            if cond_ty != Ty::Unknown && cond_ty != Ty::Bool {
                diags.push(
                    Diagnostic::error(span_of_expr(&s.cond), "condition must be of type `bool`")
                        .with_help(format!("got `{}`", cond_ty.as_str())),
                );
            }
            typeck_block(source, env, &s.body, diags);
        }
        Stmt::Empty(_) => {}
    }
}

fn typeck_let_like(
    source: &str,
    env: &mut Env,
    stmt: &LetStmt,
    is_mutable: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let declared_ty = stmt
        .ty
        .as_ref()
        .map(|t| ty_from_type_ref(source, t, TypePosition::Value, diags));

    // Like the resolver, evaluate the initializer before the binding is in scope.
    let init_ty = stmt
        .init
        .as_ref()
        .map(|e| typeck_expr(source, env, e, diags))
        .unwrap_or(Ty::Unknown);

    if !is_mutable && stmt.init.is_none() {
        diags.push(
            Diagnostic::error(stmt.span, "`const` bindings must have an initializer")
                .with_help("add `= <expr>` or change `const` to `let`"),
        );
    }

    if declared_ty.is_none() && stmt.init.is_none() {
        diags.push(
            Diagnostic::error(
                stmt.span,
                "variable declaration needs a type annotation or an initializer",
            )
            .with_help("add `: <type>` or `= <expr>`"),
        );
    }

    if let (Some(expected), Some(init)) = (declared_ty, stmt.init.as_ref()) {
        if init_ty != Ty::Unknown
            && expected != Ty::Unknown
            && expected != Ty::Void
            && !is_assignable(init_ty, expected)
        {
            diags.push(
                Diagnostic::error(span_of_expr(init), format!("type mismatch: expected `{}`, got `{}`", expected.as_str(), init_ty.as_str()))
                    .with_help("change the initializer or the declared type"),
            );
        }
    }

    let inferred_ty = match declared_ty {
        Some(t) => t,
        None => init_ty,
    };

    let Some(name) = ident_text(source, &stmt.name) else { return };

    if inferred_ty == Ty::Unknown && stmt.ty.is_none() && stmt.init.is_some() {
        diags.push(
            Diagnostic::error(
                stmt.span,
                format!("cannot infer type of `{name}` from initializer"),
            )
            .with_help("add an explicit type annotation like `: i32`"),
        );
    }

    let info = VarInfo {
        ty: inferred_ty,
        mutable: is_mutable,
        decl_span: stmt.name.span,
    };

    // If already declared (e.g. top-level predecl), update its type when we learn it.
    if let Some(existing) = env.lookup_mut(name) {
        if existing.ty == Ty::Unknown && info.ty != Ty::Unknown {
            existing.ty = info.ty;
        }
        return;
    }

    env.declare(name.to_string(), info);
}

fn typeck_expr(source: &str, env: &mut Env, expr: &Expr, diags: &mut Vec<Diagnostic>) -> Ty {
    match expr {
        Expr::Ident(ident) => {
            let Some(name) = ident_text(source, ident) else { return Ty::Unknown };
            env.lookup(name).map(|v| v.ty).unwrap_or(Ty::Unknown)
        }
        Expr::IntLit(_) => Ty::I32,
        Expr::FloatLit(_) => Ty::F64,
        Expr::StringLit(_) => Ty::String,
        Expr::BoolLit(_, _) => Ty::Bool,
        Expr::Paren { expr, .. } => typeck_expr(source, env, expr, diags),
        Expr::Unary { op, expr, span } => {
            let inner = typeck_expr(source, env, expr, diags);
            match op {
                aura_ast::UnaryOp::Neg => {
                    if inner != Ty::Unknown && !is_numeric(inner) {
                        diags.push(
                            Diagnostic::error(*span, format!("cannot apply unary `-` to `{}`", inner.as_str()))
                                .with_help("expected a numeric type"),
                        );
                        Ty::Unknown
                    } else {
                        inner
                    }
                }
                aura_ast::UnaryOp::Not => {
                    if inner != Ty::Unknown && inner != Ty::Bool {
                        diags.push(
                            Diagnostic::error(*span, format!("cannot apply unary `!` to `{}`", inner.as_str()))
                                .with_help("expected `bool`"),
                        );
                        Ty::Unknown
                    } else {
                        Ty::Bool
                    }
                }
            }
        }
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => {
            let lt = typeck_expr(source, env, left, diags);
            let rt = typeck_expr(source, env, right, diags);
            if lt == Ty::Unknown || rt == Ty::Unknown {
                return Ty::Unknown;
            }
            match op {
                aura_ast::BinaryOp::Add
                | aura_ast::BinaryOp::Sub
                | aura_ast::BinaryOp::Mul
                | aura_ast::BinaryOp::Div => {
                    if is_numeric(lt) && is_numeric(rt) {
                        unify_numeric(lt, rt)
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot apply arithmetic operator to `{}` and `{}`",
                                    lt.as_str(),
                                    rt.as_str()
                                ),
                            )
                            .with_help("expected numeric operands"),
                        );
                        Ty::Unknown
                    }
                }
                aura_ast::BinaryOp::EqEq | aura_ast::BinaryOp::NotEq => {
                    if is_comparable(lt, rt) {
                        Ty::Bool
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot compare `{}` and `{}` for equality",
                                    lt.as_str(),
                                    rt.as_str()
                                ),
                            )
                            .with_help("operands must be the same type (or both numeric)"),
                        );
                        Ty::Unknown
                    }
                }
                aura_ast::BinaryOp::Lt
                | aura_ast::BinaryOp::LtEq
                | aura_ast::BinaryOp::Gt
                | aura_ast::BinaryOp::GtEq => {
                    if is_numeric(lt) && is_numeric(rt) {
                        Ty::Bool
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot order-compare `{}` and `{}`",
                                    lt.as_str(),
                                    rt.as_str()
                                ),
                            )
                            .with_help("expected numeric operands"),
                        );
                        Ty::Unknown
                    }
                }
                aura_ast::BinaryOp::AndAnd | aura_ast::BinaryOp::OrOr => {
                    if lt == Ty::Bool && rt == Ty::Bool {
                        Ty::Bool
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot apply boolean operator to `{}` and `{}`",
                                    lt.as_str(),
                                    rt.as_str()
                                ),
                            )
                            .with_help("expected `bool` operands"),
                        );
                        Ty::Unknown
                    }
                }
            }
        }
        Expr::Assign { target, value, span } => {
            let (target_name, target_span) = match assignment_target(source, target) {
                Some(v) => v,
                None => {
                    diags.push(
                        Diagnostic::error(*span, "invalid assignment target")
                            .with_help("expected an identifier"),
                    );
                    return Ty::Unknown;
                }
            };

            let Some(var) = env.lookup(&target_name) else {
                // Resolver will report unknown identifiers; keep this as Unknown for now.
                let _ = typeck_expr(source, env, value, diags);
                return Ty::Unknown;
            };

            let target_ty = var.ty;
            let target_mutable = var.mutable;

            if !target_mutable {
                diags.push(
                    Diagnostic::error(target_span, format!("cannot assign to `const` binding `{target_name}`"))
                        .with_help("change `const` to `let` if it should be mutable"),
                );
            }

            let value_ty = typeck_expr(source, env, value, diags);
            if value_ty != Ty::Unknown
                && target_ty != Ty::Unknown
                && !is_assignable(value_ty, target_ty)
            {
                diags.push(Diagnostic::error(
                    span_of_expr(value),
                    format!(
                        "type mismatch: expected `{}`, got `{}`",
                        target_ty.as_str(),
                        value_ty.as_str()
                    ),
                ));
            }

            value_ty
        }
        Expr::Call { span, .. } => {
            diags.push(
                Diagnostic::error(*span, "call expressions are not type-checked yet")
                    .with_help("Phase 3 will type-check functions and calls"),
            );
            Ty::Unknown
        }
        Expr::Member { span, .. } => {
            diags.push(
                Diagnostic::error(*span, "member access is not type-checked yet")
                    .with_help("Phase 3 will add class/interface typing"),
            );
            Ty::Unknown
        }
    }
}

fn assignment_target(source: &str, expr: &Expr) -> Option<(String, Span)> {
    match expr {
        Expr::Ident(ident) => Some((ident_text(source, ident)?.to_string(), ident.span)),
        Expr::Paren { expr, .. } => assignment_target(source, expr),
        _ => None,
    }
}

fn span_of_expr(expr: &Expr) -> Span {
    match expr {
        Expr::Ident(i) => i.span,
        Expr::IntLit(s) => *s,
        Expr::FloatLit(s) => *s,
        Expr::StringLit(s) => *s,
        Expr::BoolLit(_, s) => *s,
        Expr::Unary { span, .. } => *span,
        Expr::Binary { span, .. } => *span,
        Expr::Assign { span, .. } => *span,
        Expr::Call { span, .. } => *span,
        Expr::Member { span, .. } => *span,
        Expr::Paren { span, .. } => *span,
    }
}

fn is_numeric(ty: Ty) -> bool {
    matches!(ty, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64)
}

fn unify_numeric(a: Ty, b: Ty) -> Ty {
    use Ty::*;
    match (a, b) {
        (F64, _) | (_, F64) => F64,
        (F32, _) | (_, F32) => F32,
        (I64, _) | (_, I64) => I64,
        _ => I32,
    }
}

fn is_comparable(a: Ty, b: Ty) -> bool {
    a == b || (is_numeric(a) && is_numeric(b))
}

fn is_assignable(from: Ty, to: Ty) -> bool {
    if from == Ty::Unknown || to == Ty::Unknown {
        return true;
    }
    if from == to {
        return true;
    }

    use Ty::*;
    match (from, to) {
        (I32, I64) => true,
        (I32, F32) | (I32, F64) => true,
        (I64, F64) | (I64, F32) => true,
        (F32, F64) => true,
        _ => false,
    }
}

fn ty_from_type_ref(
    source: &str,
    ty: &TypeRef,
    pos: TypePosition,
    diags: &mut Vec<Diagnostic>,
) -> Ty {
    let Some(name) = ident_text(source, &ty.name) else {
        diags.push(Diagnostic::error(ty.span, "invalid type reference"));
        return Ty::Unknown;
    };

    let Some(builtin) = parse_builtin_ty(name) else {
        diags.push(
            Diagnostic::error(ty.span, format!("unknown type `{name}`")).with_help(
                "built-in types: i32, i64, f32, f64, bool, string, void",
            ),
        );
        return Ty::Unknown;
    };

    if builtin == Ty::Void && pos != TypePosition::Return {
        diags.push(
            Diagnostic::error(ty.span, "type `void` is only valid as a function return type")
                .with_help("remove the annotation or use a non-void type"),
        );
    }

    builtin
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

fn ident_text<'a>(source: &'a str, ident: &Ident) -> Option<&'a str> {
    let start = ident.span.start.raw() as usize;
    let end = ident.span.end.raw() as usize;
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

    #[test]
    fn infers_let_type_from_initializer_and_allows_assignment() {
        let src = r#"
function main(): void {
  let x = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_missing_type_and_initializer() {
        let src = r#"
function main(): void {
  let x;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("type annotation or an initializer"));
    }

    #[test]
    fn allows_widening_assignment_to_annotated_type() {
        let src = r#"
function main(): void {
  let x: i64 = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_type_mismatch_in_initializer() {
        let src = r#"
function main(): void {
  let x: i32 = 1.0;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("type mismatch"));
    }

    #[test]
    fn rejects_assignment_to_const() {
        let src = r#"
function main(): void {
  const x: i32 = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("cannot assign to `const`"));
    }
}
