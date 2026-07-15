//! AST node types for Aura C0–C3f.

use std::fmt;

use crate::span::{BytePos, Span};

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub package: Path,
    /// `import demo.util` / `import demo.util as U` (alias optional; C3f uses unqualified pubs).
    pub imports: Vec<ImportDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub enums: Vec<EnumDecl>,
    pub classes: Vec<ClassDecl>,
    pub functions: Vec<FunDecl>,
    pub span: Span,
}

/// `import path` or `import path as Ident`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: Path,
    pub alias: Option<Ident>,
    /// Package that wrote this import; empty means use `File.package`.
    pub origin_package: String,
    pub span: Span,
}

/// `enum Result<T, E> { case Ok(value: T) case Err(error: E) }`
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub is_pub: bool,
    /// Declaring package; empty means use `File.package`.
    pub origin_package: String,
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
    pub is_pub: bool,
    /// Declaring package; empty means use `File.package`.
    pub origin_package: String,
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

    pub(crate) fn shift_spans(&mut self, delta: BytePos) {
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
    pub is_pub: bool,
    /// Declaring package; empty means use `File.package`.
    pub origin_package: String,
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
    pub is_pub: bool,
    /// Declaring package; empty means use `File.package`.
    pub origin_package: String,
    /// Discovered by `aura test` (RFC-011 MVP: only `@test`).
    pub is_test: bool,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: Block,
    pub span: Span,
}

/// Resolve declaring package for a top-level item.
pub fn decl_package<'a>(origin: &'a str, file_package: &'a str) -> &'a str {
    if origin.is_empty() {
        file_package
    } else {
        origin
    }
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
    /// `for (i in start..end) { … }` — exclusive Int range (C3h).
    ForRange(ForRangeStmt),
    /// `for (x in iterable) { … }` — Array elements (C3k).
    ForIn(ForInStmt),
    /// `break` — only valid inside loops (C3i).
    Break(Span),
    /// `continue` — only valid inside loops (C3i).
    Continue(Span),
    Match(MatchStmt),
    Try(TryStmt),
    Throw(ThrowStmt),
    Return(ReturnStmt),
    Expr(Expr),
}

/// `for (name in start..end)` (exclusive) or `start..=end` (inclusive, C3l).
#[derive(Debug, Clone, PartialEq)]
pub struct ForRangeStmt {
    pub name: Ident,
    pub start: Expr,
    pub end: Expr,
    /// `true` when written as `..=` (inclusive end).
    pub inclusive: bool,
    pub body: Block,
    pub span: Span,
}

/// `for (name in iterable) { body }` — currently `Array<T>` only (C3k).
#[derive(Debug, Clone, PartialEq)]
pub struct ForInStmt {
    pub name: Ident,
    pub iterable: Expr,
    pub body: Block,
    pub span: Span,
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
