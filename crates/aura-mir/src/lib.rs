//! Target-neutral mid-level IR data model.

pub use aura_ownership::OwnershipMode;
use aura_sema::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Pure,
    Async,
    Throws,
}

pub mod mir {
    use super::{Effect, OwnershipMode, Ty};
    use aura_ownership;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MirBody {
        pub name: String,
        pub locals: Vec<Local>,
        pub blocks: Vec<BasicBlock>,
        pub entry: usize,
        pub return_ty: Ty,
        pub effect: Effect,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Local {
        pub name: String,
        pub ty: Ty,
        pub ownership: OwnershipMode,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BasicBlock {
        pub statements: Vec<Statement>,
        pub terminator: Terminator,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Statement {
        Assign {
            place: Place,
            value: Rvalue,
        },
        Evaluate(Rvalue),
        Move {
            from: Place,
            to: Place,
        },
        Clone {
            from: Place,
            to: Place,
        },
        Retain {
            from: Place,
            to: Place,
        },
        ExtractVariantField {
            operand: Place,
            variant: String,
            field: String,
            to: Place,
            action: aura_ownership::Action,
        },
        LoadIndex {
            collection: Place,
            index: Place,
            to: Place,
            action: aura_ownership::Action,
        },
        StoreField {
            object: Place,
            field: String,
            value: Place,
        },
        Drop(Place),
        EnterTry {
            handler: usize,
            finally: Option<usize>,
            catch_ty: Option<Ty>,
        },
        LeaveTry,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CallTarget {
        pub name: String,
        pub package: String,
        pub type_args: Vec<Ty>,
        pub method_type_args: Vec<Ty>,
        pub is_constructor: bool,
        pub is_static: bool,
        pub is_safe: bool,
        pub variant: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Rvalue {
        Use(Place),
        ConstInt(i64),
        /// IEEE-754 bits keep MIR equality/hash behavior deterministic.
        ConstFloat(u64),
        ConstBool(bool),
        ConstString(String),
        ConstNull,
        Unary {
            op: UnaryOp,
            operand: Place,
        },
        Binary {
            op: BinaryOp,
            left: Place,
            right: Place,
        },
        Select {
            condition: Place,
            then_value: Place,
            else_value: Place,
        },
        Unwrap {
            operand: Place,
        },
        TypeTest {
            operand: Place,
            ty: Ty,
        },
        VariantTag {
            operand: Place,
        },
        Length(Place),
        Index {
            collection: Place,
            index: Place,
        },
        Field {
            object: Place,
            field: String,
        },
        Intrinsic(Intrinsic),
        /// A first-class function value identified by its source span and
        /// explicit closure captures.
        Function {
            name: String,
            captures: Vec<ClosureCapture>,
        },
        AsyncOp(AsyncOp),
        Call {
            target: CallTarget,
            args: Vec<Place>,
        },
        CallIndirect {
            callee: Place,
            args: Vec<Place>,
        },
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum UnaryOp {
        Neg,
        Not,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BinaryOp {
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
        Coalesce,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Intrinsic {
        GcCollect,
        ExceptionString,
        ExceptionInt,
        ExceptionBool,
        ExceptionObject,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AsyncOp {
        Spawn {
            body: Box<MirBody>,
            captures: Vec<SpawnCapture>,
        },
        Join(Place),
        Cancel(Place),
        ChannelCreate {
            capacity: Place,
            element_ty: Ty,
        },
        ChannelSend {
            channel: Place,
            value: Place,
        },
        ChannelReceive(Place),
        ChannelClose(Place),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SpawnCapture {
        pub source: Place,
        pub action: aura_ownership::Action,
        pub by_ref: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ClosureCapture {
        pub source: Place,
        pub ty: Ty,
        pub by_ref: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Place {
        pub local: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Terminator {
        Goto {
            target: usize,
        },
        SwitchInt {
            condition: Place,
            then_target: usize,
            else_target: usize,
        },
        SwitchTag {
            discriminant: Place,
            targets: Vec<(i64, usize)>,
            otherwise: usize,
        },
        Await {
            task: Place,
            result: Place,
            resume: usize,
            unwind: Option<usize>,
        },
        Return {
            value: Option<Place>,
        },
        Throw {
            value: Place,
            target: Option<usize>,
        },
        Cancel,
        Unreachable,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ValidationError {
        EmptyBody,
        InvalidEntry(usize),
        InvalidBlock { block: usize, target: usize },
        InvalidLocal { block: usize, local: usize },
    }

    impl MirBody {
        pub fn validate(&self) -> Result<(), ValidationError> {
            if self.blocks.is_empty() {
                return Err(ValidationError::EmptyBody);
            }
            if self.entry >= self.blocks.len() {
                return Err(ValidationError::InvalidEntry(self.entry));
            }
            for (block, body) in self.blocks.iter().enumerate() {
                for statement in &body.statements {
                    match statement {
                        Statement::Assign { place, value } => {
                            self.check_place(block, *place)?;
                            self.check_rvalue(block, value)?;
                        }
                        Statement::Evaluate(value) => self.check_rvalue(block, value)?,
                        Statement::Move { from, to }
                        | Statement::Clone { from, to }
                        | Statement::Retain { from, to } => {
                            self.check_place(block, *from)?;
                            self.check_place(block, *to)?;
                        }
                        Statement::ExtractVariantField { operand, to, .. } => {
                            self.check_place(block, *operand)?;
                            self.check_place(block, *to)?;
                        }
                        Statement::StoreField { object, value, .. } => {
                            self.check_place(block, *object)?;
                            self.check_place(block, *value)?;
                        }
                        Statement::LoadIndex {
                            collection,
                            index,
                            to,
                            ..
                        } => {
                            self.check_place(block, *collection)?;
                            self.check_place(block, *index)?;
                            self.check_place(block, *to)?;
                        }
                        Statement::Drop(place) => self.check_place(block, *place)?,
                        Statement::EnterTry {
                            handler, finally, ..
                        } => {
                            self.check_target(block, *handler)?;
                            if let Some(finally) = finally {
                                self.check_target(block, *finally)?;
                            }
                        }
                        Statement::LeaveTry => {}
                    }
                }
                match &body.terminator {
                    Terminator::Goto { target } => self.check_target(block, *target)?,
                    Terminator::SwitchInt {
                        condition,
                        then_target,
                        else_target,
                    } => {
                        self.check_place(block, *condition)?;
                        self.check_target(block, *then_target)?;
                        self.check_target(block, *else_target)?;
                    }
                    Terminator::SwitchTag {
                        discriminant,
                        targets,
                        otherwise,
                    } => {
                        self.check_place(block, *discriminant)?;
                        for (_, target) in targets {
                            self.check_target(block, *target)?;
                        }
                        self.check_target(block, *otherwise)?;
                    }
                    Terminator::Await {
                        task,
                        result,
                        resume,
                        unwind,
                    } => {
                        self.check_place(block, *task)?;
                        self.check_place(block, *result)?;
                        self.check_target(block, *resume)?;
                        if let Some(target) = unwind {
                            self.check_target(block, *target)?;
                        }
                    }
                    Terminator::Return { value } => {
                        if let Some(value) = value {
                            self.check_place(block, *value)?;
                        }
                    }
                    Terminator::Throw { value, target } => {
                        self.check_place(block, *value)?;
                        if let Some(target) = target {
                            self.check_target(block, *target)?;
                        }
                    }
                    Terminator::Cancel | Terminator::Unreachable => {}
                }
            }
            Ok(())
        }

        fn check_target(&self, block: usize, target: usize) -> Result<(), ValidationError> {
            if target >= self.blocks.len() {
                Err(ValidationError::InvalidBlock { block, target })
            } else {
                Ok(())
            }
        }

        fn check_place(&self, block: usize, place: Place) -> Result<(), ValidationError> {
            if place.local >= self.locals.len() {
                Err(ValidationError::InvalidLocal {
                    block,
                    local: place.local,
                })
            } else {
                Ok(())
            }
        }

        fn check_rvalue(&self, block: usize, value: &Rvalue) -> Result<(), ValidationError> {
            match value {
                Rvalue::Use(place) => self.check_place(block, *place)?,
                Rvalue::Call { args, .. } => {
                    for place in args {
                        self.check_place(block, *place)?;
                    }
                }
                Rvalue::CallIndirect { callee, args } => {
                    self.check_place(block, *callee)?;
                    for place in args {
                        self.check_place(block, *place)?;
                    }
                }
                Rvalue::Unary { operand, .. } => self.check_place(block, *operand)?,
                Rvalue::Binary { left, right, .. } => {
                    self.check_place(block, *left)?;
                    self.check_place(block, *right)?;
                }
                Rvalue::Select {
                    condition,
                    then_value,
                    else_value,
                } => {
                    self.check_place(block, *condition)?;
                    self.check_place(block, *then_value)?;
                    self.check_place(block, *else_value)?;
                }
                Rvalue::Unwrap { operand } => self.check_place(block, *operand)?,
                Rvalue::TypeTest { operand, .. } => self.check_place(block, *operand)?,
                Rvalue::VariantTag { operand } => self.check_place(block, *operand)?,
                Rvalue::Length(place) => self.check_place(block, *place)?,
                Rvalue::Index { collection, index } => {
                    self.check_place(block, *collection)?;
                    self.check_place(block, *index)?;
                }
                Rvalue::Field { object, .. } => self.check_place(block, *object)?,
                Rvalue::AsyncOp(operation) => match operation {
                    AsyncOp::Spawn { body, captures } => {
                        body.validate()?;
                        for capture in captures {
                            self.check_place(block, capture.source)?;
                        }
                    }
                    AsyncOp::Join(handle)
                    | AsyncOp::Cancel(handle)
                    | AsyncOp::ChannelReceive(handle)
                    | AsyncOp::ChannelClose(handle) => self.check_place(block, *handle)?,
                    AsyncOp::ChannelCreate { capacity, .. } => {
                        self.check_place(block, *capacity)?
                    }
                    AsyncOp::ChannelSend { channel, value } => {
                        self.check_place(block, *channel)?;
                        self.check_place(block, *value)?;
                    }
                },
                Rvalue::ConstInt(_)
                | Rvalue::ConstFloat(_)
                | Rvalue::ConstBool(_)
                | Rvalue::ConstString(_)
                | Rvalue::ConstNull
                | Rvalue::Function { .. }
                | Rvalue::Intrinsic(_) => {}
            }
            Ok(())
        }
    }
}
