pub use crate::compiler::frontend::token::TplPart;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Name(String, Span),
    Union(Vec<TypeExpr>, Span),
    Generic(String, Vec<TypeExpr>, Span),
    Array(Box<TypeExpr>, Span),
    Function(Vec<TypeExpr>, Box<TypeExpr>, Span),
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Name(_, s) => *s,
            TypeExpr::Union(_, s) => *s,
            TypeExpr::Generic(_, _, s) => *s,
            TypeExpr::Array(_, s) => *s,
            TypeExpr::Function(_, _, s) => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(i64, Span),
    StringLiteral(String, Span),
    Variable(String, Span),
    BinaryOp(Box<Expr>, String, Box<Expr>, Span),
    Assign(String, Box<Expr>, Span),
    Call(String, Span, Vec<Expr>, Span),
    MethodCall(Box<Expr>, String, Span, Vec<Expr>, Span),
    This(Span),
    New(String, Span, Vec<Expr>, Span),
    MemberAccess(Box<Expr>, String, Span, Span),
    MemberAssign(Box<Expr>, String, Box<Expr>, Span, Span),
    UnaryOp(String, Box<Expr>, Span),
    Throw(Box<Expr>, Span),
    TypeTest(Box<Expr>, TypeExpr, Span),
    /// Template literal: `` `Hello, ${name}!` ``
    /// Parts are alternating Str / Expr segments.
    Template(Vec<TemplatePart>, Span),
    Await(Box<Expr>, Span),
    ArrayLiteral(Vec<Expr>, Span),
    Index(Box<Expr>, Box<Expr>, Span),
    Null(Span),
    Error(Span),
}

/// A resolved template literal part (expression already parsed).
#[derive(Debug, Clone)]
pub enum TemplatePart {
    Str(String),
    Expr(Box<Expr>),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Number(_, s) => *s,
            Expr::StringLiteral(_, s) => *s,
            Expr::Variable(_, s) => *s,
            Expr::BinaryOp(_, _, _, s) => *s,
            Expr::Assign(_, _, s) => *s,
            Expr::Call(_, _, _, s) => *s,
            Expr::MethodCall(_, _, _, _, s) => *s,
            Expr::This(s) => *s,
            Expr::New(_, _, _, s) => *s,
            Expr::MemberAccess(_, _, _, s) => *s,
            Expr::MemberAssign(_, _, _, _, s) => *s,
            Expr::UnaryOp(_, _, s) => *s,
            Expr::Throw(_, s) => *s,
            Expr::TypeTest(_, _, s) => *s,
            Expr::Template(_, s) => *s,
            Expr::Await(_, s) => *s,
            Expr::ArrayLiteral(_, s) => *s,
            Expr::Index(_, _, s) => *s,
            Expr::Null(s) => *s,
            Expr::Error(s) => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
    pub value: Option<Expr>,
    pub is_static: bool,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<(String, TypeExpr)>,
    pub return_ty: TypeExpr,
    pub body: Box<Statement>,
    pub is_static: bool,
    pub is_async: bool,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ImportItem {
    Named(Vec<(String, Span)>),
    Namespace((String, Span)),
}

#[derive(Debug, Clone)]
pub enum Statement {
    VarDeclaration {
        name: String,
        name_span: Span,
        ty: Option<TypeExpr>,
        value: Expr,
        span: Span,
        doc: Option<String>,
    },
    FunctionDeclaration {
        name: String,
        name_span: Span,
        params: Vec<(String, TypeExpr)>,
        return_ty: TypeExpr,
        body: Box<Statement>,
        is_async: bool,
        span: Span,
        doc: Option<String>,
    },
    ClassDeclaration {
        name: String,
        name_span: Span,
        fields: Vec<Field>,
        methods: Vec<ClassMethod>,
        constructor: Option<ClassMethod>,
        span: Span,
        doc: Option<String>,
    },
    Return(Expr, Span),
    Print(Expr, Span),
    If {
        condition: Expr,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Box<Statement>,
        span: Span,
    },
    Block(Vec<Statement>, Span),
    Expression(Expr, Span),
    Import {
        item: ImportItem,
        path: String,
        path_span: Span,
        span: Span,
    },
    Export {
        decl: Box<Statement>,
        span: Span,
    },
    TryCatch {
        try_block: Box<Statement>,
        catch_param: Option<(String, TypeExpr)>,
        catch_block: Option<Box<Statement>>,
        finally_block: Option<Box<Statement>>,
        span: Span,
    },
    Error,
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::VarDeclaration { span, .. } => *span,
            Statement::FunctionDeclaration { span, .. } => *span,
            Statement::ClassDeclaration { span, .. } => *span,
            Statement::Return(_, s) => *s,
            Statement::Print(_, s) => *s,
            Statement::If { span, .. } => *span,
            Statement::While { span, .. } => *span,
            Statement::Block(_, s) => *s,
            Statement::Expression(_, s) => *s,
            Statement::Import { span, .. } => *span,
            Statement::Export { span, .. } => *span,
            Statement::TryCatch { span, .. } => *span,
            Statement::Error => Span::new(0, 0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
    pub file_path: String,
}
