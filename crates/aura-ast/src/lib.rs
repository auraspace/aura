//! Aura AST for compiler milestones C0–C3e (RFC-001 §6.0).

use std::fmt;

/// Byte offset into the source file (UTF-8).
pub type BytePos = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
    pub fn new(start: BytePos, end: BytePos) -> Self {
        Self { start, end }
    }

    /// Shift both endpoints by `delta` (used when concatenating multi-file packages).
    pub fn shift(self, delta: BytePos) -> Self {
        Self {
            start: self.start.saturating_add(delta),
            end: self.end.saturating_add(delta),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub package: Path,
    pub interfaces: Vec<InterfaceDecl>,
    pub enums: Vec<EnumDecl>,
    pub classes: Vec<ClassDecl>,
    pub functions: Vec<FunDecl>,
    pub span: Span,
}

/// `enum Result<T, E> { case Ok(value: T) case Err(error: E) }`
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// `case Name` or `case Name(field: Type, …)`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Ident,
    pub fields: Vec<Param>,
    pub span: Span,
}

/// `interface Name { fun m(...): T  … }` (signatures only).
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub name: Ident,
    pub methods: Vec<MethodSig>,
    pub span: Span,
}

/// Method signature without body (interfaces).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSig {
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: Span,
}

impl Path {
    /// Dotted package/path string (`demo.util`).
    pub fn display(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }

    fn shift_spans(&mut self, delta: BytePos) {
        self.span = self.span.shift(delta);
        for s in &mut self.segments {
            s.span = s.span.shift(delta);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// Type parameter with optional bounds: `T`, `T : Named`, plus `where T : Id`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Ident,
    /// Bound type names (interfaces in C2e); may come from inline `T : B` and/or `where`.
    pub bounds: Vec<Ident>,
}

/// Nominal product type: `class` (reference identity later) or `struct` (value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalKind {
    Class,
    Struct,
}

/// `class Name<T>(val x: T, …) : Iface { methods… }`
/// `struct Point(val x: Int, val y: Int) { … }`
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub kind: NominalKind,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    /// Interfaces listed after `:` (classes only; structs reject implements).
    pub implements: Vec<Ident>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FunDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub mutable: bool,
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

/// `fun name<T>(…): T { … }` / `@test fun name() { … }`
#[derive(Debug, Clone, PartialEq)]
pub struct FunDecl {
    /// Discovered by `aura test` (RFC-011 MVP: only `@test`).
    pub is_test: bool,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

/// `Box<String>?` — name, optional type arguments, optional `?`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub name: Ident,
    pub type_args: Vec<TypeRef>,
    pub nullable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Var(VarStmt),
    If(IfStmt),
    While(WhileStmt),
    Match(MatchStmt),
    Try(TryStmt),
    Throw(ThrowStmt),
    Return(ReturnStmt),
    Expr(Expr),
}

/// `throw expr`
#[derive(Debug, Clone, PartialEq)]
pub struct ThrowStmt {
    pub value: Expr,
    pub span: Span,
}

/// `try { … } catch (e: T) { … } finally { … }`
#[derive(Debug, Clone, PartialEq)]
pub struct TryStmt {
    pub try_block: Block,
    pub catch: Option<CatchClause>,
    pub finally: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause {
    pub name: Ident,
    pub ty: TypeRef,
    pub body: Block,
    pub span: Span,
}

/// `match (scrutinee) { case Ok(v) => { … } case Err(e) => { … } }`
#[derive(Debug, Clone, PartialEq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
    pub span: Span,
}

/// Variant pattern: `Ok(v)` / `Red` (unit).
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Variant {
        name: Ident,
        /// Bindings in field order (length must match variant arity).
        bindings: Vec<Ident>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarStmt {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Option<TypeRef>,
    pub init: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_block: Block,
    pub else_block: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStmt {
    pub cond: Expr,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(Ident),
    This(Span),
    Int(IntLit),
    Bool(BoolLit),
    String(StringLit),
    Null(Span),
    Call(CallExpr),
    Field(FieldExpr),
    Assign(AssignExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    /// Postfix `expr!!` — force unwrap nullable (lintable).
    ForceUnwrap(ForceUnwrapExpr),
    Group(Box<Expr>, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident(i) => i.span,
            Expr::This(s) => *s,
            Expr::Int(l) => l.span,
            Expr::Bool(l) => l.span,
            Expr::String(l) => l.span,
            Expr::Null(s) => *s,
            Expr::Call(c) => c.span,
            Expr::Field(f) => f.span,
            Expr::Assign(a) => a.span,
            Expr::Binary(b) => b.span,
            Expr::Unary(u) => u.span,
            Expr::ForceUnwrap(f) => f.span,
            Expr::Group(_, s) => *s,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForceUnwrapExpr {
    pub expr: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldExpr {
    pub object: Box<Expr>,
    pub field: Ident,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignExpr {
    pub name: Ident,
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntLit {
    pub value: i64,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoolLit {
    pub value: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringLit {
    pub value: String,
    pub span: Span,
}

/// `id(args)` or `Box<String>(args)` (type_args on generic ctor/fun).
#[derive(Debug, Clone, PartialEq)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub type_args: Vec<TypeRef>,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpr {
    pub op: BinOp,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnaryExpr {
    pub op: UnOp,
    pub expr: Box<Expr>,
    pub span: Span,
}

/// Rewrite every span in `file` by adding `delta` (multi-file virtual buffer).
pub fn shift_file_spans(file: &mut File, delta: BytePos) {
    if delta == 0 {
        return;
    }
    file.package.shift_spans(delta);
    file.span = file.span.shift(delta);
    for i in &mut file.interfaces {
        shift_interface(i, delta);
    }
    for e in &mut file.enums {
        shift_enum(e, delta);
    }
    for c in &mut file.classes {
        shift_class(c, delta);
    }
    for f in &mut file.functions {
        shift_fun(f, delta);
    }
}

fn shift_ident(i: &mut Ident, delta: BytePos) {
    i.span = i.span.shift(delta);
}

fn shift_type_param(tp: &mut TypeParam, delta: BytePos) {
    shift_ident(&mut tp.name, delta);
    for b in &mut tp.bounds {
        shift_ident(b, delta);
    }
}

fn shift_type_ref(t: &mut TypeRef, delta: BytePos) {
    shift_ident(&mut t.name, delta);
    for a in &mut t.type_args {
        shift_type_ref(a, delta);
    }
    t.span = t.span.shift(delta);
}

fn shift_param(p: &mut Param, delta: BytePos) {
    shift_ident(&mut p.name, delta);
    shift_type_ref(&mut p.ty, delta);
    p.span = p.span.shift(delta);
}

fn shift_method_sig(m: &mut MethodSig, delta: BytePos) {
    shift_ident(&mut m.name, delta);
    for p in &mut m.params {
        shift_param(p, delta);
    }
    if let Some(rt) = &mut m.return_type {
        shift_type_ref(rt, delta);
    }
    m.span = m.span.shift(delta);
}

fn shift_interface(i: &mut InterfaceDecl, delta: BytePos) {
    shift_ident(&mut i.name, delta);
    for m in &mut i.methods {
        shift_method_sig(m, delta);
    }
    i.span = i.span.shift(delta);
}

fn shift_enum_variant(v: &mut EnumVariant, delta: BytePos) {
    shift_ident(&mut v.name, delta);
    for f in &mut v.fields {
        shift_param(f, delta);
    }
    v.span = v.span.shift(delta);
}

fn shift_enum(e: &mut EnumDecl, delta: BytePos) {
    shift_ident(&mut e.name, delta);
    for tp in &mut e.type_params {
        shift_type_param(tp, delta);
    }
    for v in &mut e.variants {
        shift_enum_variant(v, delta);
    }
    e.span = e.span.shift(delta);
}

fn shift_field(f: &mut FieldDecl, delta: BytePos) {
    shift_ident(&mut f.name, delta);
    shift_type_ref(&mut f.ty, delta);
    f.span = f.span.shift(delta);
}

fn shift_class(c: &mut ClassDecl, delta: BytePos) {
    shift_ident(&mut c.name, delta);
    for tp in &mut c.type_params {
        shift_type_param(tp, delta);
    }
    for i in &mut c.implements {
        shift_ident(i, delta);
    }
    for f in &mut c.fields {
        shift_field(f, delta);
    }
    for m in &mut c.methods {
        shift_fun(m, delta);
    }
    c.span = c.span.shift(delta);
}

fn shift_fun(f: &mut FunDecl, delta: BytePos) {
    shift_ident(&mut f.name, delta);
    for tp in &mut f.type_params {
        shift_type_param(tp, delta);
    }
    for p in &mut f.params {
        shift_param(p, delta);
    }
    if let Some(rt) = &mut f.return_type {
        shift_type_ref(rt, delta);
    }
    shift_block(&mut f.body, delta);
    f.span = f.span.shift(delta);
}

fn shift_block(b: &mut Block, delta: BytePos) {
    for s in &mut b.stmts {
        shift_stmt(s, delta);
    }
    b.span = b.span.shift(delta);
}

fn shift_stmt(s: &mut Stmt, delta: BytePos) {
    match s {
        Stmt::Var(v) => {
            shift_ident(&mut v.name, delta);
            if let Some(t) = &mut v.ty {
                shift_type_ref(t, delta);
            }
            shift_expr(&mut v.init, delta);
            v.span = v.span.shift(delta);
        }
        Stmt::If(i) => {
            shift_expr(&mut i.cond, delta);
            shift_block(&mut i.then_block, delta);
            if let Some(e) = &mut i.else_block {
                shift_block(e, delta);
            }
            i.span = i.span.shift(delta);
        }
        Stmt::While(w) => {
            shift_expr(&mut w.cond, delta);
            shift_block(&mut w.body, delta);
            w.span = w.span.shift(delta);
        }
        Stmt::Match(m) => {
            shift_expr(&mut m.scrutinee, delta);
            for a in &mut m.arms {
                shift_pattern(&mut a.pattern, delta);
                shift_block(&mut a.body, delta);
                a.span = a.span.shift(delta);
            }
            m.span = m.span.shift(delta);
        }
        Stmt::Try(t) => {
            shift_block(&mut t.try_block, delta);
            if let Some(c) = &mut t.catch {
                shift_ident(&mut c.name, delta);
                shift_type_ref(&mut c.ty, delta);
                shift_block(&mut c.body, delta);
                c.span = c.span.shift(delta);
            }
            if let Some(f) = &mut t.finally {
                shift_block(f, delta);
            }
            t.span = t.span.shift(delta);
        }
        Stmt::Throw(t) => {
            shift_expr(&mut t.value, delta);
            t.span = t.span.shift(delta);
        }
        Stmt::Return(r) => {
            if let Some(v) = &mut r.value {
                shift_expr(v, delta);
            }
            r.span = r.span.shift(delta);
        }
        Stmt::Expr(e) => shift_expr(e, delta),
    }
}

fn shift_pattern(p: &mut Pattern, delta: BytePos) {
    match p {
        Pattern::Variant {
            name,
            bindings,
            span,
        } => {
            shift_ident(name, delta);
            for b in bindings {
                shift_ident(b, delta);
            }
            *span = span.shift(delta);
        }
    }
}

fn shift_expr(e: &mut Expr, delta: BytePos) {
    match e {
        Expr::Ident(i) => shift_ident(i, delta),
        Expr::This(s) => *s = s.shift(delta),
        Expr::Int(l) => l.span = l.span.shift(delta),
        Expr::Bool(l) => l.span = l.span.shift(delta),
        Expr::String(l) => l.span = l.span.shift(delta),
        Expr::Null(s) => *s = s.shift(delta),
        Expr::Call(c) => {
            shift_expr(&mut c.callee, delta);
            for t in &mut c.type_args {
                shift_type_ref(t, delta);
            }
            for a in &mut c.args {
                shift_expr(a, delta);
            }
            c.span = c.span.shift(delta);
        }
        Expr::Field(f) => {
            shift_expr(&mut f.object, delta);
            shift_ident(&mut f.field, delta);
            f.span = f.span.shift(delta);
        }
        Expr::Assign(a) => {
            shift_ident(&mut a.name, delta);
            shift_expr(&mut a.value, delta);
            a.span = a.span.shift(delta);
        }
        Expr::Binary(b) => {
            shift_expr(&mut b.left, delta);
            shift_expr(&mut b.right, delta);
            b.span = b.span.shift(delta);
        }
        Expr::Unary(u) => {
            shift_expr(&mut u.expr, delta);
            u.span = u.span.shift(delta);
        }
        Expr::ForceUnwrap(f) => {
            shift_expr(&mut f.expr, delta);
            f.span = f.span.shift(delta);
        }
        Expr::Group(inner, s) => {
            shift_expr(inner, delta);
            *s = s.shift(delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_display() {
        let p = Path {
            segments: vec![
                Ident {
                    name: "demo".into(),
                    span: Span::new(0, 4),
                },
                Ident {
                    name: "util".into(),
                    span: Span::new(5, 9),
                },
            ],
            span: Span::new(0, 9),
        };
        assert_eq!(p.display(), "demo.util");
    }

    #[test]
    fn span_shift() {
        let s = Span::new(3, 7).shift(10);
        assert_eq!(s, Span::new(13, 17));
    }
}
