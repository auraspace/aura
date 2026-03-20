pub use crate::compiler::frontend::token::TplPart;

#[derive(Debug, Clone)]
pub enum DocComment {
    Line(String),
    Block(String),
}

impl DocComment {
    pub fn content(&self) -> String {
        match self {
            DocComment::Line(s) | DocComment::Block(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessModifier {
    Public,
    Private,
    Protected,
}

impl AccessModifier {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessModifier::Public => "public",
            AccessModifier::Private => "private",
            AccessModifier::Protected => "protected",
        }
    }
}

impl Default for AccessModifier {
    fn default() -> Self {
        AccessModifier::Public
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Name(String, Span),
    Union(Vec<TypeExpr>, Span),
    Generic(String, Vec<TypeExpr>, Span),
    Array(Box<TypeExpr>, Span),
    Function(Vec<TypeParam>, Vec<TypeExpr>, Box<TypeExpr>, Span),
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Name(_, s) => *s,
            TypeExpr::Union(_, s) => *s,
            TypeExpr::Generic(_, _, s) => *s,
            TypeExpr::Array(_, s) => *s,
            TypeExpr::Function(_, _, _, s) => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(i64, Span),
    Float(f64, Span),
    StringLiteral(String, Span),
    Variable(String, Span),
    BinaryOp(Box<Expr>, String, Box<Expr>, Span),
    Assign(String, Box<Expr>, Span),
    Call(String, Vec<TypeExpr>, Span, Vec<Expr>, Span),
    MethodCall(Box<Expr>, String, Vec<TypeExpr>, Span, Vec<Expr>, Span),
    This(Span),
    New(String, Vec<TypeExpr>, Span, Vec<Expr>, Span),
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
    IndexAssign(Box<Expr>, Box<Expr>, Box<Expr>, Span),
    SuperCall(Vec<Expr>, Span),
    Super(Span),
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
            Expr::Float(_, s) => *s,
            Expr::StringLiteral(_, s) => *s,
            Expr::Variable(_, s) => *s,
            Expr::BinaryOp(_, _, _, s) => *s,
            Expr::Assign(_, _, s) => *s,
            Expr::Call(_, _, _, _, s) => *s,
            Expr::MethodCall(_, _, _, _, _, s) => *s,
            Expr::This(s) => *s,
            Expr::New(_, _, _, _, s) => *s,
            Expr::MemberAccess(_, _, _, s) => *s,
            Expr::MemberAssign(_, _, _, _, s) => *s,
            Expr::UnaryOp(_, _, s) => *s,
            Expr::Throw(_, s) => *s,
            Expr::TypeTest(_, _, s) => *s,
            Expr::Template(_, s) => *s,
            Expr::Await(_, s) => *s,
            Expr::ArrayLiteral(_, s) => *s,
            Expr::Index(_, _, s) => *s,
            Expr::IndexAssign(_, _, _, s) => *s,
            Expr::SuperCall(_, s) => *s,
            Expr::Super(s) => *s,
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
    pub is_readonly: bool,
    pub access: AccessModifier,
    pub span: Span,
    pub doc: Option<DocComment>,
}

#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<(String, TypeExpr)>,
    pub return_ty: TypeExpr,
    pub body: Box<Statement>,
    pub is_static: bool,
    pub is_async: bool,
    pub is_override: bool,
    pub is_abstract: bool,
    pub access: AccessModifier,
    pub span: Span,
    pub doc: Option<DocComment>,
}

#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: String,
    pub name_span: Span,
    pub value: Option<Expr>, // Explicit value (int or string)
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub name_span: Span,
    pub members: Vec<EnumMember>,
    pub span: Span,
    pub doc: Option<DocComment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: String,
    pub constraint: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<Field>,
    pub methods: Vec<ClassMethod>,
    pub span: Span,
    pub doc: Option<DocComment>,
}

#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
    pub span: Span,
    pub doc: Option<DocComment>,
}

#[derive(Debug, Clone)]
pub enum ImportItem {
    Named(Vec<(String, Span)>),
    Namespace((String, Span)),
}

#[derive(Debug, Clone)]
pub enum Statement {
    Enum(EnumDecl),
    Interface(InterfaceDecl),
    TypeAlias(TypeAliasDecl),
    VarDeclaration {
        name: String,
        name_span: Span,
        ty: Option<TypeExpr>,
        value: Expr,
        is_const: bool,
        span: Span,
        doc: Option<DocComment>,
    },
    FunctionDeclaration {
        name: String,
        name_span: Span,
        type_params: Vec<TypeParam>,
        params: Vec<(String, TypeExpr)>,
        return_ty: TypeExpr,
        body: Box<Statement>,
        is_async: bool,
        span: Span,
        doc: Option<DocComment>,
    },
    ClassDeclaration {
        name: String,
        name_span: Span,
        type_params: Vec<TypeParam>,
        extends: Option<TypeExpr>,
        implements: Vec<TypeExpr>,
        fields: Vec<Field>,
        methods: Vec<ClassMethod>,
        constructor: Option<ClassMethod>,
        is_abstract: bool,
        span: Span,
        doc: Option<DocComment>,
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
    For {
        initializer: Option<Box<Statement>>,
        condition: Option<Expr>,
        increment: Option<Expr>,
        body: Box<Statement>,
        span: Span,
    },
    ForOf {
        variable: String,
        variable_span: Span,
        is_const: bool,
        iterable: Expr,
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
    Comment(String, Span),
    RegularBlockComment(String, Span),
    Empty(Span),
    Error,
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::Enum(d) => d.span,
            Statement::Interface(d) => d.span,
            Statement::TypeAlias(d) => d.span,
            Statement::VarDeclaration { span, .. } => *span,
            Statement::FunctionDeclaration { span, .. } => *span,
            Statement::ClassDeclaration { span, .. } => *span,
            Statement::Return(_, s) => *s,
            Statement::Print(_, s) => *s,
            Statement::If { span, .. } => *span,
            Statement::While { span, .. } => *span,
            Statement::For { span, .. } => *span,
            Statement::ForOf { span, .. } => *span,
            Statement::Block(_, s) => *s,
            Statement::Expression(_, s) => *s,
            Statement::Import { span, .. } => *span,
            Statement::Export { span, .. } => *span,
            Statement::TryCatch { span, .. } => *span,
            Statement::Comment(_, s) => *s,
            Statement::RegularBlockComment(_, s) => *s,
            Statement::Empty(s) => *s,
            Statement::Error => Span::new(0, 0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
    pub file_path: String,
}
