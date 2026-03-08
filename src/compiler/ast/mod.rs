#[derive(Debug, Clone)]
pub enum TypeExpr {
    Name(String),
    Union(Vec<TypeExpr>),
    Generic(String, Vec<TypeExpr>),
    Array(Box<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(i32),
    StringLiteral(String),
    Variable(String),
    BinaryOp(Box<Expr>, String, Box<Expr>),
    Assign(String, Box<Expr>),
    Call(String, Vec<Expr>),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    This,
    New(String, Vec<Expr>),
    MemberAccess(Box<Expr>, String),
    MemberAssign(Box<Expr>, String, Box<Expr>),
    TypeTest(Box<Expr>, TypeExpr),
    Error,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub params: Vec<(String, TypeExpr)>,
    pub return_ty: TypeExpr,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    VarDeclaration {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    FunctionDeclaration {
        name: String,
        params: Vec<(String, TypeExpr)>,
        return_ty: TypeExpr,
        body: Box<Statement>,
    },
    ClassDeclaration {
        name: String,
        fields: Vec<Field>,
        methods: Vec<ClassMethod>,
        constructor: Option<ClassMethod>,
    },
    Return(Expr),
    Print(Expr),
    If {
        condition: Expr,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
    },
    While {
        condition: Expr,
        body: Box<Statement>,
    },
    Block(Vec<Statement>),
    Expression(Expr),
    Error,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}
