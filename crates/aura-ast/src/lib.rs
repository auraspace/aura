use aura_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub items: Vec<TopLevel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopLevel {
    Import(ImportDecl),
    Export(ExportDecl),
    Function(FunctionDecl),
    Class(ClassDecl),
    Interface(InterfaceDecl),
    Stmt(Stmt),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDecl {
    pub clause: ImportClause,
    /// Span of the string literal token (includes the surrounding quotes).
    pub from_path: Span,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportClause {
    Named(Vec<Ident>),
    Default(Ident),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportDecl {
    pub item: Option<ExportedDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportedDecl {
    Function(FunctionDecl),
    Class(ClassDecl),
    Interface(InterfaceDecl),
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
pub struct ClassDecl {
    pub name: Ident,
    pub extends: Option<TypeRef>,
    pub implements: Vec<TypeRef>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<MethodDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceDecl {
    pub name: Ident,
    pub methods: Vec<InterfaceMethodDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceMethodDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodDecl {
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
    Throw(ThrowStmt),
    Expr(ExprStmt),
    Block(Block),
    If(IfStmt),
    While(WhileStmt),
    Try(TryStmt),
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
pub struct ThrowStmt {
    pub value: Expr,
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
pub struct CatchClause {
    pub binding: Ident,
    pub ty: Option<TypeRef>,
    pub block: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryStmt {
    pub try_block: Block,
    pub catch: Option<CatchClause>,
    pub finally_block: Option<Block>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Ident(Ident),
    This(Span),
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
    New {
        class: Ident,
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

#[cfg(test)]
mod tests {
    use super::*;
    use aura_span::{BytePos, Span};

    #[test]
    fn constructs_nested_ast_nodes() {
        let name_span = Span::new(BytePos::new(0), BytePos::new(3));
        let type_span = Span::new(BytePos::new(5), BytePos::new(8));
        let body_span = Span::new(BytePos::new(10), BytePos::new(20));
        let function_span = Span::new(BytePos::new(0), BytePos::new(20));

        let function = FunctionDecl {
            name: Ident { span: name_span },
            params: vec![Param {
                name: Ident { span: name_span },
                ty: TypeRef {
                    name: Ident { span: type_span },
                    span: type_span,
                },
                span: Span::new(BytePos::new(0), BytePos::new(8)),
            }],
            return_type: Some(TypeRef {
                name: Ident { span: type_span },
                span: type_span,
            }),
            body: Block {
                stmts: vec![Stmt::Empty(body_span)],
                span: body_span,
            },
            span: function_span,
        };

        let program = Program {
            items: vec![TopLevel::Function(function.clone())],
        };

        assert_eq!(program.items.len(), 1);
        assert_eq!(function, function.clone());
        assert_eq!(program.items[0], TopLevel::Function(function));
    }
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
    Mod,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
}
