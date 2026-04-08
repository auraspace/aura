pub mod dump;
mod lower;
pub use dump::dump_mir;
pub use lower::lower_program;

use aura_ast::{BinaryOp, UnaryOp};
use aura_span::Span;
use aura_typeck::Ty;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MirProgram {
    pub functions: Vec<MirFunction>,
    pub classes: HashMap<String, MirClass>,
    pub interfaces: HashMap<String, MirInterface>,
    pub method_slots: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MirClass {
    pub name: String,
    pub extends: Option<String>,
    pub fields: HashMap<String, Ty>,
    pub field_order: Vec<String>,
    pub methods: HashMap<String, MirFunction>,
}

#[derive(Debug, Clone)]
pub struct MirInterface {
    pub methods: HashMap<String, aura_typeck::MethodSig>,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub name: String,
    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub cleanup_regions: Vec<CleanupRegion>,
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub ty: Ty,
    pub name: Option<String>,
    pub span: Span,
    pub kind: LocalKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Arg,
    Var,
    Temp,
    Return,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: usize,
    pub statements: Vec<Statement>,
    pub terminator: Option<Terminator>,
}

#[derive(Debug, Clone)]
pub struct CleanupRegion {
    pub try_block: usize,
    pub catch_block: Option<usize>,
    pub finally_block: Option<usize>,
    pub after_block: usize,
    pub edges: Vec<CleanupEdge>,
}

#[derive(Debug, Clone)]
pub struct CleanupEdge {
    pub from_block: usize,
    pub to_block: usize,
    pub reason: CleanupReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupReason {
    Normal,
    Return,
    Throw,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Assign(Lvalue, Rvalue),
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Goto(usize),
    SwitchInt {
        discr: Operand,
        targets: Vec<(i64, usize)>,
        otherwise: usize,
    },
    Return(Option<Operand>),
    Call {
        callee: Operand,
        args: Vec<Operand>,
        destination: Lvalue,
        target: usize,
    },
    Unreachable,
}

#[derive(Debug, Clone)]
pub enum Lvalue {
    Local(usize),
    Field(Box<Lvalue>, String),
}

#[derive(Debug, Clone)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinaryOp, Operand, Operand),
    UnaryOp(UnaryOp, Operand),
    Ref(Lvalue),
}

#[derive(Debug, Clone)]
pub enum Operand {
    Copy(Lvalue),
    Move(Lvalue),
    Constant(Constant),
}

#[derive(Debug, Clone)]
pub enum Constant {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}
