use aura_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub items: Vec<TopLevel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopLevel {
    Function(FunctionDecl),
    Stmt(Stmt),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeRef {
    pub name: Ident,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Let(LetStmt),
    Const(LetStmt),
    Return(ReturnStmt),
    Expr(ExprStmt),
    Block(Block),
    If(IfStmt),
    While(WhileStmt),
    Empty(Span),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LetStmt {
    pub name: Ident,
    pub ty: Option<TypeRef>,
    pub init: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_block: Block,
    pub else_block: Option<Block>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhileStmt {
    pub cond: Expr,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Ident(Ident),
    IntLit(Span),
    FloatLit(Span),
    StringLit(Span),
    BoolLit(bool, Span),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Member {
        object: Box<Expr>,
        field: Ident,
        span: Span,
    },
    Paren {
        expr: Box<Expr>,
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
}
