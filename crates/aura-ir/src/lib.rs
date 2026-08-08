//! Backend-neutral compiler IR contracts.
//!
//! CheckedIr is the first stable boundary after semantic checking. Backends
//! consume its declarations, type facts, monomorphization requests, and
//! ownership/effect facts instead of re-running language lowering.
//!
//! The source field is a compatibility bridge for the alpha C backend. New
//! backends must not depend on it; it will disappear once expression and
//! statement lowering has moved into MIR.

use std::collections::HashMap;

use aura_ast::{Block, ForeignLinkKind, Span, Stmt, TypeRef};
use aura_sema::{AttributeMetadata, CallInstantiation, CheckedFile, ExpansionMetadata, Ty};

pub mod generic_lowering;

pub mod mir {
    use super::{ownership, Effect, OwnershipMode, Ty};

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
            action: ownership::Action,
        },
        LoadIndex {
            collection: Place,
            index: Place,
            to: Place,
            action: ownership::Action,
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
        AsyncOp(AsyncOp),
        Call {
            target: CallTarget,
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
        pub action: ownership::Action,
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
                | Rvalue::Intrinsic(_) => {}
            }
            Ok(())
        }
    }
}

fn collect_spawn_state_machines(
    body: &mir::MirBody,
    output: &mut Vec<state_machine::StateMachine>,
) {
    for block in &body.blocks {
        for statement in &block.statements {
            let value = match statement {
                mir::Statement::Assign { value, .. } => Some(value),
                mir::Statement::Evaluate(value) => Some(value),
                _ => None,
            };
            let Some(mir::Rvalue::AsyncOp(mir::AsyncOp::Spawn { body, .. })) = value else {
                continue;
            };
            if let Ok(machine) = state_machine::StateMachine::from_mir(body) {
                output.push(machine);
            }
            collect_spawn_state_machines(body, output);
        }
    }
}

/// Backend-neutral state-machine form derived from validated MIR. Future
/// backends consume suspension edges and frame layout without parsing target
/// code or depending on the alpha C compatibility model.
pub mod state_machine {
    use super::mir::{MirBody, Terminator};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct StateMachine {
        pub function: String,
        pub entry: usize,
        pub frame_locals: Vec<usize>,
        pub states: Vec<State>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct State {
        pub block: usize,
        pub successors: Vec<usize>,
        pub suspension: Option<Suspension>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Suspension {
        pub task_local: usize,
        pub result_local: usize,
        pub resume: usize,
        pub unwind: Option<usize>,
        pub ownership: Vec<OwnershipTransfer>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OwnershipTransfer {
        pub local: usize,
        pub action: super::ownership::Action,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum BuildError {
        InvalidMir,
    }

    impl StateMachine {
        pub fn from_mir(body: &MirBody) -> Result<Self, BuildError> {
            body.validate().map_err(|_| BuildError::InvalidMir)?;
            let mut frame_locals = Vec::new();
            let states = body
                .blocks
                .iter()
                .enumerate()
                .map(|(block, basic)| {
                    let mut successors = Vec::new();
                    let suspension = match &basic.terminator {
                        Terminator::Goto { target } => {
                            successors.push(*target);
                            None
                        }
                        Terminator::SwitchInt {
                            then_target,
                            else_target,
                            ..
                        } => {
                            successors.extend([*then_target, *else_target]);
                            None
                        }
                        Terminator::SwitchTag {
                            targets, otherwise, ..
                        } => {
                            successors.extend(targets.iter().map(|(_, target)| *target));
                            successors.push(*otherwise);
                            None
                        }
                        Terminator::Await {
                            task,
                            result,
                            resume,
                            unwind,
                        } => {
                            successors.push(*resume);
                            if let Some(unwind) = unwind {
                                successors.push(*unwind);
                            }
                            frame_locals.extend([task.local, result.local]);
                            Some(Suspension {
                                task_local: task.local,
                                result_local: result.local,
                                resume: *resume,
                                unwind: *unwind,
                                ownership: body
                                    .locals
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(local, value)| {
                                        let action =
                                            super::ownership::plan_for_ty(&value.ty).across_suspend;
                                        (!matches!(
                                            action,
                                            super::ownership::Action::Copy
                                                | super::ownership::Action::Noop
                                        ))
                                        .then_some(OwnershipTransfer { local, action })
                                    })
                                    .collect(),
                            })
                        }
                        Terminator::Throw { target, .. } => {
                            if let Some(target) = target {
                                successors.push(*target);
                            }
                            None
                        }
                        Terminator::Return { .. }
                        | Terminator::Cancel
                        | Terminator::Unreachable => None,
                    };
                    State {
                        block,
                        successors,
                        suspension,
                    }
                })
                .collect::<Vec<_>>();
            if states.iter().any(|state| state.suspension.is_some()) {
                // Conservative frame placement is intentional until the
                // liveness pass is introduced: every local may be observed
                // by a resumed block, so none may remain stack-only.
                frame_locals.extend(0..body.locals.len());
            }
            frame_locals.sort_unstable();
            frame_locals.dedup();
            Ok(Self {
                function: body.name.clone(),
                entry: body.entry,
                frame_locals,
                states,
            })
        }
    }
}

pub mod lowering {
    use std::collections::{BTreeSet, HashMap};

    use aura_ast::{AsyncExpr, AsyncFunDecl, Block, Expr, Ident, Span, Stmt, VarStmt};
    use aura_sema::{subst_ty, type_subst_map, CheckedFile, Ty};

    use super::{
        mir::{
            AsyncOp, BasicBlock, BinaryOp, CallTarget, Intrinsic, Local, MirBody, Place, Rvalue,
            SpawnCapture, Statement, Terminator, UnaryOp,
        },
        ownership, Effect,
    };

    #[derive(Debug, Clone)]
    enum IterableAccess {
        Builtin,
        Protocol {
            len: ProtocolLength,
            get: Box<CallTarget>,
        },
    }

    #[derive(Debug, Clone)]
    enum ProtocolLength {
        Method(CallTarget),
        Field(String),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum LowerError {
        Unsupported { span: Span, construct: &'static str },
        MissingType { span: Span },
        UnknownLocal { span: Span, name: String },
        InvalidControlFlow,
    }

    /// Lower the linear/await subset into typed MIR.
    ///
    /// This is intentionally a real frontend lowering API, not a C helper:
    /// it has no target-language strings and its output can be validated or
    /// consumed by any backend. Branch/loop lowering is added on top of this
    /// contract in the next slice.
    pub fn lower_async_body(
        name: &str,
        body: &Block,
        params: &[(String, Ty)],
        return_ty: Ty,
        checked: Option<&CheckedFile>,
    ) -> Result<MirBody, LowerError> {
        lower_body(name, body, params, return_ty, checked, Effect::Async)
    }

    /// Lower a semantically checked body without selecting a target backend.
    /// Async lowering is the common path; the effect is supplied by the
    /// checked function signature so sync bodies share the same MIR contract.
    pub fn lower_body(
        name: &str,
        body: &Block,
        params: &[(String, Ty)],
        return_ty: Ty,
        checked: Option<&CheckedFile>,
        effect: Effect,
    ) -> Result<MirBody, LowerError> {
        let mut locals = Vec::new();
        let mut local_ids = HashMap::new();
        for (param, ty) in params {
            let id = locals.len();
            local_ids.insert(param.clone(), id);
            locals.push(Local {
                name: param.clone(),
                ty: ty.clone(),
                ownership: ownership::mode_for_ty(ty),
            });
        }
        let mut blocks = vec![BasicBlock {
            statements: Vec::new(),
            terminator: Terminator::Unreachable,
        }];
        let mut current = 0;

        for stmt in &body.stmts {
            match stmt {
                Stmt::Try(value)
                    if value.try_block.stmts.len() == 1
                        && matches!(value.try_block.stmts[0], Stmt::Throw(_))
                        && value
                            .catch
                            .as_ref()
                            .is_some_and(|catch| catch.body.stmts.len() <= 1) =>
                {
                    let handler = blocks.len() + 1;
                    let finally_target = handler + 1;
                    let join = finally_target + 1;
                    let try_block = blocks.len();
                    blocks.push(BasicBlock {
                        statements: vec![Statement::EnterTry {
                            handler,
                            finally: value.finally.as_ref().map(|_| finally_target),
                            catch_ty: value
                                .catch
                                .as_ref()
                                .and_then(|catch| crate::type_ref_builtin(&catch.ty)),
                        }],
                        terminator: Terminator::Unreachable,
                    });
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Goto { target: join },
                    });
                    blocks[current].terminator = Terminator::Goto { target: try_block };
                    let Stmt::Throw(throw) = &value.try_block.stmts[0] else {
                        unreachable!("guarded above");
                    };
                    let thrown = place_or_temp(
                        &throw.value,
                        &mut locals,
                        &mut blocks[try_block].statements,
                        &local_ids,
                        checked,
                    )?;
                    blocks[try_block].terminator = Terminator::Throw {
                        value: thrown,
                        target: Some(handler),
                    };
                    if let Some(catch) = &value.catch {
                        lower_branch_terminal(
                            &catch.body,
                            handler,
                            finally_target,
                            &mut blocks,
                            &mut locals,
                            &local_ids,
                            checked,
                            None,
                        )?;
                    } else {
                        blocks[handler].terminator = Terminator::Goto {
                            target: finally_target,
                        };
                    }
                    current = join;
                }
                Stmt::Try(value)
                    if value.try_block.stmts.len() == 2
                        && matches!(value.try_block.stmts[0], Stmt::Var(_))
                        && matches!(value.try_block.stmts[1], Stmt::Throw(_))
                        && value.finally.is_none()
                        && value
                            .catch
                            .as_ref()
                            .is_some_and(|catch| catch.body.stmts.len() <= 1) =>
                {
                    let Stmt::Var(binding) = &value.try_block.stmts[0] else {
                        unreachable!("guarded above");
                    };
                    let Expr::Async(AsyncExpr::Await(await_expr)) = &binding.init else {
                        return Err(LowerError::Unsupported {
                            span: binding.span,
                            construct: "try await initializer",
                        });
                    };
                    let result_ty = binding
                        .ty
                        .as_ref()
                        .and_then(crate::type_ref_builtin)
                        .or_else(|| {
                            checked.and_then(|file| {
                                file.expr_tys
                                    .get(&(binding.init.span().start, binding.init.span().end))
                                    .cloned()
                            })
                        })
                        .ok_or(LowerError::MissingType { span: binding.span })?;
                    let handler = blocks.len() + 1;
                    let resume = handler + 1;
                    let join = resume + 1;
                    let try_block = blocks.len();
                    blocks.push(BasicBlock {
                        statements: vec![Statement::EnterTry {
                            handler,
                            finally: None,
                            catch_ty: value
                                .catch
                                .as_ref()
                                .and_then(|catch| crate::type_ref_builtin(&catch.ty)),
                        }],
                        terminator: Terminator::Unreachable,
                    });
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks[current].terminator = Terminator::Goto { target: try_block };

                    let local = locals.len();
                    locals.push(Local {
                        name: binding.name.name.clone(),
                        ty: result_ty.clone(),
                        ownership: ownership::mode_for_ty(&result_ty),
                    });
                    let mut try_bindings = local_ids.clone();
                    try_bindings.insert(binding.name.name.clone(), local);
                    let task = lower_await_operand(
                        &await_expr.operand,
                        &result_ty,
                        &local_ids,
                        &mut locals,
                        &mut blocks[try_block].statements,
                        checked,
                    )?;
                    blocks[try_block].terminator = Terminator::Await {
                        task,
                        result: Place { local },
                        resume,
                        unwind: Some(handler),
                    };
                    let Stmt::Throw(throw) = &value.try_block.stmts[1] else {
                        unreachable!("guarded above");
                    };
                    let thrown = place_or_temp(
                        &throw.value,
                        &mut locals,
                        &mut blocks[resume].statements,
                        &try_bindings,
                        checked,
                    )?;
                    blocks[resume].terminator = Terminator::Throw {
                        value: thrown,
                        target: Some(handler),
                    };
                    if let Some(catch) = &value.catch {
                        lower_branch_terminal(
                            &catch.body,
                            handler,
                            join,
                            &mut blocks,
                            &mut locals,
                            &local_ids,
                            checked,
                            None,
                        )?;
                    } else {
                        blocks[handler].terminator = Terminator::Goto { target: join };
                    }
                    current = join;
                }
                Stmt::While(value) => {
                    let head = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let body_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Goto { target: head },
                    });
                    let exit = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks[current].terminator = Terminator::Goto { target: head };
                    let condition = place_or_temp(
                        &value.cond,
                        &mut locals,
                        &mut blocks[head].statements,
                        &local_ids,
                        checked,
                    )?;
                    blocks[head].terminator = Terminator::SwitchInt {
                        condition,
                        then_target: body_target,
                        else_target: exit,
                    };
                    lower_loop_body(
                        &value.body,
                        body_target,
                        head,
                        &mut blocks,
                        &mut locals,
                        &local_ids,
                        checked,
                    )?;
                    current = exit;
                }
                Stmt::ForRange(value) => {
                    let loop_var = locals.len();
                    locals.push(Local {
                        name: value.name.name.clone(),
                        ty: Ty::Int,
                        ownership: ownership::mode_for_ty(&Ty::Int),
                    });
                    let mut loop_bindings = local_ids.clone();
                    loop_bindings.insert(value.name.name.clone(), loop_var);
                    let start = place_or_temp(
                        &value.start,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        checked,
                    )?;
                    let end = place_or_temp(
                        &value.end,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        checked,
                    )?;
                    blocks[current].statements.push(Statement::Assign {
                        place: Place { local: loop_var },
                        value: Rvalue::Use(start),
                    });
                    let head = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let body_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Goto { target: head },
                    });
                    let increment = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let exit = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let condition_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_for_condition_{condition_local}"),
                        ty: Ty::Bool,
                        ownership: ownership::mode_for_ty(&Ty::Bool),
                    });
                    blocks[head].statements.push(Statement::Assign {
                        place: Place {
                            local: condition_local,
                        },
                        value: Rvalue::Binary {
                            op: if value.inclusive {
                                BinaryOp::Le
                            } else {
                                BinaryOp::Lt
                            },
                            left: Place { local: loop_var },
                            right: end,
                        },
                    });
                    blocks[head].terminator = Terminator::SwitchInt {
                        condition: Place {
                            local: condition_local,
                        },
                        then_target: body_target,
                        else_target: exit,
                    };
                    blocks[current].terminator = Terminator::Goto { target: head };
                    lower_loop_body(
                        &value.body,
                        body_target,
                        increment,
                        &mut blocks,
                        &mut locals,
                        &loop_bindings,
                        checked,
                    )?;
                    let one_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_for_one_{one_local}"),
                        ty: Ty::Int,
                        ownership: ownership::mode_for_ty(&Ty::Int),
                    });
                    blocks[increment].statements.push(Statement::Assign {
                        place: Place { local: one_local },
                        value: Rvalue::ConstInt(1),
                    });
                    blocks[increment].statements.push(Statement::Assign {
                        place: Place { local: loop_var },
                        value: Rvalue::Binary {
                            op: BinaryOp::Add,
                            left: Place { local: loop_var },
                            right: Place { local: one_local },
                        },
                    });
                    blocks[increment].terminator = Terminator::Goto { target: head };
                    current = exit;
                }
                Stmt::ForIn(value) => {
                    let iterable_ty = checked
                        .and_then(|file| {
                            file.expr_tys
                                .get(&(value.iterable.span().start, value.iterable.span().end))
                        })
                        .cloned()
                        .ok_or(LowerError::MissingType {
                            span: value.iterable.span(),
                        })?;
                    let (element_ty, iterable_access) = match &iterable_ty {
                        Ty::String => (Ty::Int, IterableAccess::Builtin),
                        Ty::ClassApp { name, args } if name == "Array" && args.len() == 1 => {
                            (args[0].clone(), IterableAccess::Builtin)
                        }
                        Ty::Interface(_) | Ty::InterfaceApp { .. } => {
                            let (element_ty, len, get) = protocol_methods(
                                &iterable_ty,
                                checked.ok_or(LowerError::MissingType {
                                    span: value.iterable.span(),
                                })?,
                                false,
                                value.span,
                            )?;
                            (
                                element_ty,
                                IterableAccess::Protocol {
                                    len,
                                    get: Box::new(get),
                                },
                            )
                        }
                        Ty::Class(_) | Ty::ClassApp { .. } => {
                            let (element_ty, len, get) = protocol_methods(
                                &iterable_ty,
                                checked.ok_or(LowerError::MissingType {
                                    span: value.iterable.span(),
                                })?,
                                true,
                                value.span,
                            )?;
                            (
                                element_ty,
                                IterableAccess::Protocol {
                                    len,
                                    get: Box::new(get),
                                },
                            )
                        }
                        _ => {
                            return Err(LowerError::Unsupported {
                                span: value.span,
                                construct: "for-in iterable protocol",
                            });
                        }
                    };
                    let iterable = place_or_temp(
                        &value.iterable,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        checked,
                    )?;
                    let index_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_for_in_index_{index_local}"),
                        ty: Ty::Int,
                        ownership: ownership::mode_for_ty(&Ty::Int),
                    });
                    blocks[current].statements.push(Statement::Assign {
                        place: Place { local: index_local },
                        value: Rvalue::ConstInt(0),
                    });
                    let head = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let body_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let increment = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let exit = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let length_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_for_in_length_{length_local}"),
                        ty: Ty::Int,
                        ownership: ownership::mode_for_ty(&Ty::Int),
                    });
                    blocks[current].terminator = Terminator::Goto { target: head };
                    let length_value = match &iterable_access {
                        IterableAccess::Builtin => Rvalue::Length(iterable),
                        IterableAccess::Protocol {
                            len: ProtocolLength::Method(len),
                            ..
                        } => Rvalue::Call {
                            target: len.clone(),
                            args: vec![iterable],
                        },
                        IterableAccess::Protocol {
                            len: ProtocolLength::Field(field),
                            ..
                        } => Rvalue::Field {
                            object: iterable,
                            field: field.clone(),
                        },
                    };
                    blocks[head].statements.push(Statement::Assign {
                        place: Place {
                            local: length_local,
                        },
                        value: length_value,
                    });
                    let condition_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_for_in_condition_{condition_local}"),
                        ty: Ty::Bool,
                        ownership: ownership::mode_for_ty(&Ty::Bool),
                    });
                    blocks[head].statements.push(Statement::Assign {
                        place: Place {
                            local: condition_local,
                        },
                        value: Rvalue::Binary {
                            op: BinaryOp::Lt,
                            left: Place { local: index_local },
                            right: Place {
                                local: length_local,
                            },
                        },
                    });
                    blocks[head].terminator = Terminator::SwitchInt {
                        condition: Place {
                            local: condition_local,
                        },
                        then_target: body_target,
                        else_target: exit,
                    };
                    let binding_local = locals.len();
                    locals.push(Local {
                        name: value.name.name.clone(),
                        ty: element_ty.clone(),
                        ownership: ownership::mode_for_ty(&element_ty),
                    });
                    match &iterable_access {
                        IterableAccess::Builtin => {
                            blocks[body_target].statements.push(Statement::LoadIndex {
                                collection: iterable,
                                index: Place { local: index_local },
                                to: Place {
                                    local: binding_local,
                                },
                                action: iteration_action(&element_ty),
                            });
                        }
                        IterableAccess::Protocol { get, .. } => {
                            let item_local = locals.len();
                            locals.push(Local {
                                name: format!("__mir_for_in_item_{item_local}"),
                                ty: element_ty.clone(),
                                ownership: ownership::mode_for_ty(&element_ty),
                            });
                            blocks[body_target].statements.push(Statement::Assign {
                                place: Place { local: item_local },
                                value: Rvalue::Call {
                                    target: (**get).clone(),
                                    args: vec![iterable, Place { local: index_local }],
                                },
                            });
                            bind_loaded_value(
                                &mut blocks[body_target].statements,
                                item_local,
                                binding_local,
                                iteration_action(&element_ty),
                            );
                        }
                    }
                    let mut loop_bindings = local_ids.clone();
                    loop_bindings.insert(value.name.name.clone(), binding_local);
                    lower_branch_terminal(
                        &value.body,
                        body_target,
                        increment,
                        &mut blocks,
                        &mut locals,
                        &loop_bindings,
                        checked,
                        Some((exit, increment)),
                    )?;
                    if matches!(blocks[body_target].terminator, Terminator::Goto { .. }) {
                        blocks[body_target].statements.push(Statement::Drop(Place {
                            local: binding_local,
                        }));
                    }
                    let one_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_for_in_one_{one_local}"),
                        ty: Ty::Int,
                        ownership: ownership::mode_for_ty(&Ty::Int),
                    });
                    blocks[increment].statements.push(Statement::Assign {
                        place: Place { local: one_local },
                        value: Rvalue::ConstInt(1),
                    });
                    blocks[increment].statements.push(Statement::Assign {
                        place: Place { local: index_local },
                        value: Rvalue::Binary {
                            op: BinaryOp::Add,
                            left: Place { local: index_local },
                            right: Place { local: one_local },
                        },
                    });
                    blocks[increment].terminator = Terminator::Goto { target: head };
                    current = exit;
                }
                Stmt::If(value) => {
                    let condition = place_or_temp(
                        &value.cond,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        checked,
                    )?;
                    let then_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let else_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let join_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks[current].terminator = Terminator::SwitchInt {
                        condition,
                        then_target,
                        else_target,
                    };
                    lower_branch_terminal(
                        &value.then_block,
                        then_target,
                        join_target,
                        &mut blocks,
                        &mut locals,
                        &local_ids,
                        checked,
                        None,
                    )?;
                    if let Some(else_block) = &value.else_block {
                        lower_branch_terminal(
                            else_block,
                            else_target,
                            join_target,
                            &mut blocks,
                            &mut locals,
                            &local_ids,
                            checked,
                            None,
                        )?;
                    } else {
                        blocks[else_target].terminator = Terminator::Goto {
                            target: join_target,
                        };
                    }
                    current = join_target;
                }
                Stmt::Match(value) => {
                    let checked_file = checked.ok_or(LowerError::Unsupported {
                        span: value.span,
                        construct: "match without semantic facts",
                    })?;
                    let scrutinee_ty = checked_file
                        .expr_tys
                        .get(&(value.scrutinee.span().start, value.scrutinee.span().end))
                        .cloned()
                        .ok_or(LowerError::MissingType {
                            span: value.scrutinee.span(),
                        })?;
                    let enum_name = match &scrutinee_ty {
                        Ty::Enum(name) | Ty::EnumApp { name, .. } => name
                            .rsplit("::")
                            .next()
                            .unwrap_or(name)
                            .split('@')
                            .next()
                            .unwrap_or(name),
                        _ => {
                            return Err(LowerError::Unsupported {
                                span: value.span,
                                construct: "match non-enum scrutinee",
                            });
                        }
                    };
                    let enum_decl = checked_file
                        .ast
                        .enums
                        .iter()
                        .find(|decl| decl.name.name == enum_name)
                        .ok_or(LowerError::Unsupported {
                            span: value.span,
                            construct: "unknown match enum",
                        })?;
                    let scrutinee = place_or_temp(
                        &value.scrutinee,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        Some(checked_file),
                    )?;
                    let tag_local = locals.len();
                    locals.push(Local {
                        name: format!("__mir_match_tag_{tag_local}"),
                        ty: Ty::Int,
                        ownership: ownership::mode_for_ty(&Ty::Int),
                    });
                    blocks[current].statements.push(Statement::Assign {
                        place: Place { local: tag_local },
                        value: Rvalue::VariantTag { operand: scrutinee },
                    });
                    let join = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let mut targets = Vec::new();
                    for arm in &value.arms {
                        let aura_ast::Pattern::Variant { name, bindings, .. } = &arm.pattern;
                        let variant = enum_decl
                            .variants
                            .iter()
                            .find(|variant| variant.name.name == name.name)
                            .ok_or(LowerError::Unsupported {
                                span: arm.span,
                                construct: "unknown match variant",
                            })?;
                        if bindings.len() != variant.fields.len() {
                            return Err(LowerError::Unsupported {
                                span: arm.span,
                                construct: "match payload arity",
                            });
                        }
                        let tag = enum_decl
                            .variants
                            .iter()
                            .position(|variant| variant.name.name == name.name)
                            .ok_or(LowerError::Unsupported {
                                span: arm.span,
                                construct: "unknown match variant",
                            })? as i64;
                        let arm_block = blocks.len();
                        blocks.push(BasicBlock {
                            statements: Vec::new(),
                            terminator: Terminator::Unreachable,
                        });
                        let mut arm_bindings = local_ids.clone();
                        for (binding, field) in bindings.iter().zip(variant.fields.iter()) {
                            let field_ty =
                                enum_field_ty(&field.ty, enum_decl, &scrutinee_ty, checked_file)
                                    .ok_or(LowerError::Unsupported {
                                        span: field.span,
                                        construct: "generic match payload",
                                    })?;
                            let ownership = ownership::mode_for_ty(&field_ty);
                            let local = locals.len();
                            locals.push(Local {
                                name: binding.name.clone(),
                                ty: field_ty,
                                ownership,
                            });
                            arm_bindings.insert(binding.name.clone(), local);
                            blocks[arm_block]
                                .statements
                                .push(Statement::ExtractVariantField {
                                    operand: scrutinee,
                                    variant: name.name.clone(),
                                    field: field.name.name.clone(),
                                    to: Place { local },
                                    action: ownership::plan_for_ty(&locals[local].ty).bind,
                                });
                        }
                        lower_branch_terminal(
                            &arm.body,
                            arm_block,
                            join,
                            &mut blocks,
                            &mut locals,
                            &arm_bindings,
                            Some(checked_file),
                            None,
                        )?;
                        targets.push((tag, arm_block));
                    }
                    blocks[current].terminator = Terminator::SwitchTag {
                        discriminant: Place { local: tag_local },
                        targets,
                        otherwise: join,
                    };
                    current = join;
                }
                Stmt::Var(value) => {
                    let ty = value
                        .ty
                        .as_ref()
                        .and_then(|type_ref| {
                            checked.and_then(|file| type_ref_to_ty(type_ref, &HashMap::new(), file))
                        })
                        .or_else(|| value.ty.as_ref().and_then(crate::type_ref_builtin))
                        .or_else(|| {
                            checked.and_then(|file| {
                                file.expr_tys
                                    .get(&(value.init.span().start, value.init.span().end))
                                    .cloned()
                            })
                        })
                        .ok_or(LowerError::MissingType { span: value.span })?;
                    let id = locals.len();
                    local_ids.insert(value.name.name.clone(), id);
                    locals.push(Local {
                        name: value.name.name.clone(),
                        ty: ty.clone(),
                        ownership: ownership::mode_for_ty(&ty),
                    });
                    if let Expr::Async(AsyncExpr::Await(await_expr)) = &value.init {
                        let task = lower_await_operand(
                            &await_expr.operand,
                            &ty,
                            &local_ids,
                            &mut locals,
                            &mut blocks[current].statements,
                            checked,
                        )?;
                        let next = blocks.len();
                        blocks.push(BasicBlock {
                            statements: Vec::new(),
                            terminator: Terminator::Unreachable,
                        });
                        blocks[current].terminator = Terminator::Await {
                            task,
                            result: Place { local: id },
                            resume: next,
                            unwind: None,
                        };
                        current = next;
                    } else {
                        let rvalue = lower_rvalue(
                            &value.init,
                            &mut locals,
                            &mut blocks[current].statements,
                            &local_ids,
                            checked,
                        )?;
                        let destination = Place { local: id };
                        match (rvalue, ownership::plan_for_ty(&ty).bind) {
                            (Rvalue::Use(from), ownership::Action::Move) => {
                                blocks[current].statements.push(Statement::Move {
                                    from,
                                    to: destination,
                                });
                            }
                            (Rvalue::Use(from), ownership::Action::Clone) => {
                                blocks[current].statements.push(Statement::Clone {
                                    from,
                                    to: destination,
                                });
                            }
                            (value, _) => blocks[current].statements.push(Statement::Assign {
                                place: destination,
                                value,
                            }),
                        }
                    }
                }
                Stmt::Return(value) => {
                    let place = value
                        .value
                        .as_ref()
                        .map(|expr| {
                            place_or_temp(
                                expr,
                                &mut locals,
                                &mut blocks[current].statements,
                                &local_ids,
                                checked,
                            )
                        })
                        .transpose()?;
                    append_scope_exit_drops(&mut blocks[current].statements, &locals, 0, place);
                    blocks[current].terminator = Terminator::Return { value: place };
                    break;
                }
                Stmt::Throw(value) => {
                    let place = place_or_temp(
                        &value.value,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        checked,
                    )?;
                    append_scope_exit_drops(
                        &mut blocks[current].statements,
                        &locals,
                        0,
                        Some(place),
                    );
                    blocks[current].terminator = Terminator::Throw {
                        value: place,
                        target: None,
                    };
                    break;
                }
                Stmt::Expr(expr) => {
                    if let Expr::Assign(assign) = expr {
                        lower_assignment(
                            assign,
                            &mut blocks[current].statements,
                            &local_ids,
                            &mut locals,
                            checked,
                        )?;
                        continue;
                    }
                    if let Expr::Async(AsyncExpr::Await(await_expr)) = expr {
                        let result_ty = checked
                            .and_then(|file| {
                                file.expr_tys
                                    .get(&(await_expr.span.start, await_expr.span.end))
                                    .cloned()
                            })
                            .unwrap_or(Ty::Unit);
                        let task = lower_await_operand(
                            &await_expr.operand,
                            &result_ty,
                            &local_ids,
                            &mut locals,
                            &mut blocks[current].statements,
                            checked,
                        )?;
                        let result = locals.len();
                        let result_ownership = ownership::mode_for_ty(&result_ty);
                        locals.push(Local {
                            name: format!("__await_discard_{result}"),
                            ty: result_ty,
                            ownership: result_ownership,
                        });
                        let resume = blocks.len();
                        blocks.push(BasicBlock {
                            statements: Vec::new(),
                            terminator: Terminator::Unreachable,
                        });
                        blocks[current].terminator = Terminator::Await {
                            task,
                            result: Place { local: result },
                            resume,
                            unwind: None,
                        };
                        current = resume;
                        continue;
                    }
                    let value = lower_rvalue(
                        expr,
                        &mut locals,
                        &mut blocks[current].statements,
                        &local_ids,
                        checked,
                    )?;
                    blocks[current].statements.push(Statement::Evaluate(value));
                }
                _ => {
                    return Err(LowerError::Unsupported {
                        span: stmt_span(stmt),
                        construct: "branch or loop",
                    });
                }
            }
        }

        if matches!(blocks[current].terminator, Terminator::Unreachable) {
            append_scope_exit_drops(&mut blocks[current].statements, &locals, 0, None);
            blocks[current].terminator = Terminator::Return { value: None };
        }
        let body = MirBody {
            name: name.into(),
            locals,
            blocks,
            entry: 0,
            return_ty,
            effect,
        };
        body.validate()
            .map_err(|_| LowerError::InvalidControlFlow)?;
        Ok(body)
    }

    fn lower_rvalue(
        expr: &Expr,
        locals_out: &mut Vec<Local>,
        statements: &mut Vec<Statement>,
        locals: &HashMap<String, usize>,
        checked: Option<&CheckedFile>,
    ) -> Result<Rvalue, LowerError> {
        match expr {
            Expr::Int(value) => Ok(Rvalue::ConstInt(value.value)),
            Expr::Float(value) => Ok(Rvalue::ConstFloat(value.value.to_bits())),
            Expr::Bool(value) => Ok(Rvalue::ConstBool(value.value)),
            Expr::String(value) => Ok(Rvalue::ConstString(value.value.clone())),
            Expr::Null(_) => Ok(Rvalue::ConstNull),
            Expr::Ident(value) => match place_for_expr(expr, locals) {
                Ok(place) => Ok(Rvalue::Use(place)),
                Err(_) => {
                    if let Some(constant) = checked.and_then(|file| {
                        file.ast
                            .consts
                            .iter()
                            .find(|constant| constant.name.name == value.name)
                    }) {
                        return lower_rvalue(
                            &constant.value,
                            locals_out,
                            statements,
                            locals,
                            checked,
                        );
                    }
                    let object = place_for_expr(&Expr::This(value.span), locals)?;
                    let object_ty = locals_out[object.local].ty.clone();
                    if field_type_for_ty(&object_ty, &value.name, checked).is_some() {
                        Ok(Rvalue::Field {
                            object,
                            field: value.name.clone(),
                        })
                    } else {
                        Err(LowerError::UnknownLocal {
                            span: value.span,
                            name: value.name.clone(),
                        })
                    }
                }
            },
            Expr::This(_) => Ok(Rvalue::Use(place_for_expr(expr, locals)?)),
            Expr::Group(value, _) => lower_rvalue(value, locals_out, statements, locals, checked),
            Expr::ForceUnwrap(value) => Ok(Rvalue::Unwrap {
                operand: place_or_temp(&value.expr, locals_out, statements, locals, checked)?,
            }),
            Expr::Is(value) => {
                let checked = checked.ok_or(LowerError::MissingType { span: value.span })?;
                let ty = type_ref_to_ty(&value.ty, &HashMap::new(), checked)
                    .ok_or(LowerError::MissingType { span: value.span })?;
                Ok(Rvalue::TypeTest {
                    operand: place_or_temp(
                        &value.expr,
                        locals_out,
                        statements,
                        locals,
                        Some(checked),
                    )?,
                    ty,
                })
            }
            Expr::Field(value) => {
                let object = place_or_temp(
                    value.object.as_ref(),
                    locals_out,
                    statements,
                    locals,
                    checked,
                )?;
                Ok(Rvalue::Field {
                    object,
                    field: value.field.name.clone(),
                })
            }
            Expr::Unary(value) => {
                let operand =
                    place_or_temp(value.expr.as_ref(), locals_out, statements, locals, checked)?;
                Ok(Rvalue::Unary {
                    op: match value.op {
                        aura_ast::UnOp::Neg => UnaryOp::Neg,
                        aura_ast::UnOp::Not => UnaryOp::Not,
                    },
                    operand,
                })
            }
            Expr::Binary(value) => {
                let left =
                    place_or_temp(value.left.as_ref(), locals_out, statements, locals, checked)?;
                let right = place_or_temp(
                    value.right.as_ref(),
                    locals_out,
                    statements,
                    locals,
                    checked,
                )?;
                Ok(Rvalue::Binary {
                    op: match value.op {
                        aura_ast::BinOp::Add => BinaryOp::Add,
                        aura_ast::BinOp::Sub => BinaryOp::Sub,
                        aura_ast::BinOp::Mul => BinaryOp::Mul,
                        aura_ast::BinOp::Div => BinaryOp::Div,
                        aura_ast::BinOp::Rem => BinaryOp::Rem,
                        aura_ast::BinOp::Eq => BinaryOp::Eq,
                        aura_ast::BinOp::Ne => BinaryOp::Ne,
                        aura_ast::BinOp::Lt => BinaryOp::Lt,
                        aura_ast::BinOp::Le => BinaryOp::Le,
                        aura_ast::BinOp::Gt => BinaryOp::Gt,
                        aura_ast::BinOp::Ge => BinaryOp::Ge,
                        aura_ast::BinOp::And => BinaryOp::And,
                        aura_ast::BinOp::Or => BinaryOp::Or,
                        aura_ast::BinOp::Coalesce => BinaryOp::Coalesce,
                    },
                    left,
                    right,
                })
            }
            Expr::If(value) => {
                if value.then_block.stmts.len() != 1 || value.else_block.stmts.len() != 1 {
                    return Err(LowerError::Unsupported {
                        span: value.span,
                        construct: "multi-statement expression if",
                    });
                }
                let Stmt::Expr(then_expr) = &value.then_block.stmts[0] else {
                    return Err(LowerError::Unsupported {
                        span: value.span,
                        construct: "non-expression if branch",
                    });
                };
                let Stmt::Expr(else_expr) = &value.else_block.stmts[0] else {
                    return Err(LowerError::Unsupported {
                        span: value.span,
                        construct: "non-expression if branch",
                    });
                };
                if !pure_select_expr(then_expr) || !pure_select_expr(else_expr) {
                    return Err(LowerError::Unsupported {
                        span: value.span,
                        construct: "effectful expression if branch",
                    });
                }
                let condition =
                    place_or_temp(&value.cond, locals_out, statements, locals, checked)?;
                let then_value = place_or_temp(then_expr, locals_out, statements, locals, checked)?;
                let else_value = place_or_temp(else_expr, locals_out, statements, locals, checked)?;
                Ok(Rvalue::Select {
                    condition,
                    then_value,
                    else_value,
                })
            }
            Expr::Async(async_expr) => match async_expr {
                AsyncExpr::Spawn(value) => {
                    let result_ty = checked
                        .and_then(|file| {
                            file.expr_tys
                                .get(&(value.span.start, value.span.end))
                                .cloned()
                        })
                        .ok_or(LowerError::MissingType { span: value.span })?;
                    let Ty::TaskHandle(result_ty) = result_ty else {
                        return Err(LowerError::Unsupported {
                            span: value.span,
                            construct: "spawn result type",
                        });
                    };
                    let capture_names = spawn_capture_names(&value.body, locals);
                    let params = capture_names
                        .iter()
                        .map(|name| {
                            let local =
                                locals.get(name).copied().ok_or(LowerError::UnknownLocal {
                                    span: value.span,
                                    name: name.clone(),
                                })?;
                            Ok((name.clone(), locals_out[local].ty.clone()))
                        })
                        .collect::<Result<Vec<_>, LowerError>>()?;
                    let captures = capture_names
                        .iter()
                        .map(|name| {
                            let local =
                                locals.get(name).copied().ok_or(LowerError::UnknownLocal {
                                    span: value.span,
                                    name: name.clone(),
                                })?;
                            Ok(SpawnCapture {
                                source: Place { local },
                                action: ownership::plan_for_ty(&locals_out[local].ty)
                                    .across_suspend,
                            })
                        })
                        .collect::<Result<Vec<_>, LowerError>>()?;
                    let body = lower_body(
                        &format!("__spawn_{}", value.span.start),
                        &value.body,
                        &params,
                        *result_ty,
                        checked,
                        Effect::Async,
                    )?;
                    Ok(Rvalue::AsyncOp(AsyncOp::Spawn {
                        body: Box::new(body),
                        captures,
                    }))
                }
                AsyncExpr::Await(await_expr) => Err(LowerError::Unsupported {
                    span: await_expr.span,
                    construct: "await expression without statement continuation",
                }),
                AsyncExpr::Join(value) => Ok(Rvalue::AsyncOp(AsyncOp::Join(place_or_temp(
                    &value.handle,
                    locals_out,
                    statements,
                    locals,
                    checked,
                )?))),
                AsyncExpr::Cancel(value) => Ok(Rvalue::AsyncOp(AsyncOp::Cancel(place_or_temp(
                    &value.handle,
                    locals_out,
                    statements,
                    locals,
                    checked,
                )?))),
                AsyncExpr::ChannelCreate(value) => {
                    let element_ty = checked
                        .and_then(|file| type_ref_to_ty(&value.element_type, &HashMap::new(), file))
                        .ok_or(LowerError::MissingType { span: value.span })?;
                    Ok(Rvalue::AsyncOp(AsyncOp::ChannelCreate {
                        capacity: place_or_temp(
                            &value.capacity,
                            locals_out,
                            statements,
                            locals,
                            checked,
                        )?,
                        element_ty,
                    }))
                }
                AsyncExpr::ChannelSend(value) => Ok(Rvalue::AsyncOp(AsyncOp::ChannelSend {
                    channel: place_or_temp(
                        &value.channel,
                        locals_out,
                        statements,
                        locals,
                        checked,
                    )?,
                    value: place_or_temp(&value.value, locals_out, statements, locals, checked)?,
                })),
                AsyncExpr::ChannelReceive(value) => Ok(Rvalue::AsyncOp(AsyncOp::ChannelReceive(
                    place_or_temp(&value.channel, locals_out, statements, locals, checked)?,
                ))),
                AsyncExpr::ChannelClose(value) => Ok(Rvalue::AsyncOp(AsyncOp::ChannelClose(
                    place_or_temp(&value.channel, locals_out, statements, locals, checked)?,
                ))),
            },
            Expr::Call(value) => {
                if matches!(
                    value.callee.as_ref(),
                    Expr::Ident(function) if function.name == "gc_collect"
                ) && value.args.is_empty()
                {
                    return Ok(Rvalue::Intrinsic(Intrinsic::GcCollect));
                }
                let (function_name, receiver) = match value.callee.as_ref() {
                    Expr::Ident(function) => (function.name.clone(), None),
                    Expr::Field(field) => (
                        field.field.name.clone(),
                        Some(place_or_temp(
                            field.object.as_ref(),
                            locals_out,
                            statements,
                            locals,
                            checked,
                        )?),
                    ),
                    _ => {
                        return Err(LowerError::Unsupported {
                            span: value.span,
                            construct: "non-name call",
                        });
                    }
                };
                let mut args = value
                    .args
                    .iter()
                    .map(|arg| place_or_temp(arg, locals_out, statements, locals, checked))
                    .collect::<Result<Vec<_>, _>>()?;
                let target = checked
                    .and_then(|file| file.call_instantiations.get(&value.span.start))
                    .map(|call| CallTarget {
                        // Nested calls can share a start offset; the AST
                        // callee remains authoritative for non-constructors.
                        name: if call.is_constructor {
                            call.name.clone()
                        } else {
                            function_name.clone()
                        },
                        package: call.package.clone(),
                        type_args: call.type_args.clone(),
                        method_type_args: call.method_type_args.clone(),
                        is_constructor: call.is_constructor,
                        is_static: call.is_static,
                        variant: call.variant.clone(),
                    })
                    .unwrap_or_else(|| CallTarget {
                        name: function_name.clone(),
                        package: String::new(),
                        type_args: Vec::new(),
                        method_type_args: Vec::new(),
                        is_constructor: false,
                        is_static: false,
                        variant: None,
                    });
                if let (Some(receiver), false) = (receiver, target.is_static) {
                    args.insert(0, receiver);
                }
                Ok(Rvalue::Call { target, args })
            }
            _ => Err(LowerError::Unsupported {
                span: expr.span(),
                construct: "expression",
            }),
        }
    }

    fn spawn_capture_names(block: &Block, available: &HashMap<String, usize>) -> Vec<String> {
        fn expr_refs(
            expr: &Expr,
            locals: &BTreeSet<String>,
            available: &HashMap<String, usize>,
            captures: &mut BTreeSet<String>,
        ) {
            match expr {
                Expr::Ident(value) => {
                    if available.contains_key(&value.name) && !locals.contains(&value.name) {
                        captures.insert(value.name.clone());
                    }
                }
                Expr::Call(value) => {
                    expr_refs(&value.callee, locals, available, captures);
                    for arg in &value.args {
                        expr_refs(arg, locals, available, captures);
                    }
                }
                Expr::Field(value) => expr_refs(&value.object, locals, available, captures),
                Expr::Assign(value) => {
                    if available.contains_key(&value.name.name)
                        && !locals.contains(&value.name.name)
                    {
                        captures.insert(value.name.name.clone());
                    }
                    expr_refs(&value.value, locals, available, captures);
                }
                Expr::Binary(value) => {
                    expr_refs(&value.left, locals, available, captures);
                    expr_refs(&value.right, locals, available, captures);
                }
                Expr::Unary(value) => expr_refs(&value.expr, locals, available, captures),
                Expr::ForceUnwrap(value) => expr_refs(&value.expr, locals, available, captures),
                Expr::Is(value) => expr_refs(&value.expr, locals, available, captures),
                Expr::Group(value, _) => expr_refs(value, locals, available, captures),
                Expr::If(value) => {
                    expr_refs(&value.cond, locals, available, captures);
                    block_refs(&value.then_block, locals, available, captures);
                    block_refs(&value.else_block, locals, available, captures);
                }
                Expr::Lambda(value) => match &value.body {
                    aura_ast::LambdaBody::Expr(body) => {
                        expr_refs(body, locals, available, captures)
                    }
                    aura_ast::LambdaBody::Block(body) => {
                        block_refs(body, locals, available, captures)
                    }
                },
                Expr::Async(value) => match value {
                    AsyncExpr::Await(value) => {
                        expr_refs(&value.operand, locals, available, captures)
                    }
                    AsyncExpr::Spawn(value) => block_refs(&value.body, locals, available, captures),
                    AsyncExpr::Join(value) => expr_refs(&value.handle, locals, available, captures),
                    AsyncExpr::Cancel(value) => {
                        expr_refs(&value.handle, locals, available, captures)
                    }
                    AsyncExpr::ChannelCreate(value) => {
                        expr_refs(&value.capacity, locals, available, captures)
                    }
                    AsyncExpr::ChannelSend(value) => {
                        expr_refs(&value.channel, locals, available, captures);
                        expr_refs(&value.value, locals, available, captures);
                    }
                    AsyncExpr::ChannelReceive(value) => {
                        expr_refs(&value.channel, locals, available, captures)
                    }
                    AsyncExpr::ChannelClose(value) => {
                        expr_refs(&value.channel, locals, available, captures)
                    }
                },
                Expr::This(_)
                | Expr::Int(_)
                | Expr::Float(_)
                | Expr::Bool(_)
                | Expr::String(_)
                | Expr::Null(_) => {}
            }
        }

        fn block_refs(
            block: &Block,
            inherited: &BTreeSet<String>,
            available: &HashMap<String, usize>,
            captures: &mut BTreeSet<String>,
        ) {
            let mut locals = inherited.clone();
            for statement in &block.stmts {
                match statement {
                    Stmt::Var(value) => {
                        expr_refs(&value.init, &locals, available, captures);
                        locals.insert(value.name.name.clone());
                    }
                    Stmt::If(value) => {
                        expr_refs(&value.cond, &locals, available, captures);
                        block_refs(&value.then_block, &locals, available, captures);
                        if let Some(else_block) = &value.else_block {
                            block_refs(else_block, &locals, available, captures);
                        }
                    }
                    Stmt::While(value) => {
                        expr_refs(&value.cond, &locals, available, captures);
                        block_refs(&value.body, &locals, available, captures);
                    }
                    Stmt::ForRange(value) => {
                        expr_refs(&value.start, &locals, available, captures);
                        expr_refs(&value.end, &locals, available, captures);
                        let mut body_locals = locals.clone();
                        body_locals.insert(value.name.name.clone());
                        block_refs(&value.body, &body_locals, available, captures);
                    }
                    Stmt::ForIn(value) => {
                        expr_refs(&value.iterable, &locals, available, captures);
                        let mut body_locals = locals.clone();
                        body_locals.insert(value.name.name.clone());
                        block_refs(&value.body, &body_locals, available, captures);
                    }
                    Stmt::Match(value) => {
                        expr_refs(&value.scrutinee, &locals, available, captures);
                        for arm in &value.arms {
                            block_refs(&arm.body, &locals, available, captures);
                        }
                    }
                    Stmt::Try(value) => {
                        block_refs(&value.try_block, &locals, available, captures);
                        if let Some(catch) = &value.catch {
                            block_refs(&catch.body, &locals, available, captures);
                        }
                        if let Some(finally) = &value.finally {
                            block_refs(finally, &locals, available, captures);
                        }
                    }
                    Stmt::Throw(value) => expr_refs(&value.value, &locals, available, captures),
                    Stmt::Return(value) => {
                        if let Some(value) = &value.value {
                            expr_refs(value, &locals, available, captures);
                        }
                    }
                    Stmt::Expr(value) => expr_refs(value, &locals, available, captures),
                    Stmt::Break(_) | Stmt::Continue(_) => {}
                }
            }
        }

        let mut captures = BTreeSet::new();
        block_refs(block, &BTreeSet::new(), available, &mut captures);
        captures.into_iter().collect()
    }

    fn pure_select_expr(expr: &Expr) -> bool {
        match expr {
            Expr::Ident(_)
            | Expr::This(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Null(_) => true,
            Expr::Field(value) => pure_select_expr(&value.object),
            Expr::Unary(value) => pure_select_expr(&value.expr),
            Expr::Binary(value) => pure_select_expr(&value.left) && pure_select_expr(&value.right),
            Expr::Group(value, _)
            | Expr::ForceUnwrap(aura_ast::ForceUnwrapExpr { expr: value, .. }) => {
                pure_select_expr(value)
            }
            Expr::Is(_)
            | Expr::Call(_)
            | Expr::Assign(_)
            | Expr::If(_)
            | Expr::Lambda(_)
            | Expr::Async(_) => false,
        }
    }

    fn lower_assignment(
        assignment: &aura_ast::AssignExpr,
        statements: &mut Vec<Statement>,
        bindings: &HashMap<String, usize>,
        locals: &mut Vec<Local>,
        checked: Option<&CheckedFile>,
    ) -> Result<(), LowerError> {
        let Some(destination) = bindings
            .get(&assignment.name.name)
            .copied()
            .map(|local| Place { local })
        else {
            let object = bindings
                .get("this")
                .copied()
                .map(|local| Place { local })
                .ok_or_else(|| LowerError::UnknownLocal {
                    span: assignment.name.span,
                    name: assignment.name.name.clone(),
                })?;
            let object_ty = locals[object.local].ty.clone();
            if field_type_for_ty(&object_ty, &assignment.name.name, checked).is_none() {
                return Err(LowerError::UnknownLocal {
                    span: assignment.name.span,
                    name: assignment.name.name.clone(),
                });
            }
            let value = place_or_temp(&assignment.value, locals, statements, bindings, checked)?;
            statements.push(Statement::StoreField {
                object,
                field: assignment.name.name.clone(),
                value,
            });
            return Ok(());
        };
        let value = lower_rvalue(&assignment.value, locals, statements, bindings, checked)?;
        let action = ownership::plan_for_ty(&locals[destination.local].ty).assign;
        if let Rvalue::Use(source) = value {
            if source == destination {
                return Ok(());
            }
            if ownership::plan_for_ty(&locals[destination.local].ty).scope_exit
                == ownership::Action::Drop
            {
                statements.push(Statement::Drop(destination));
            }
            match action {
                ownership::Action::Move => statements.push(Statement::Move {
                    from: source,
                    to: destination,
                }),
                ownership::Action::Clone => statements.push(Statement::Clone {
                    from: source,
                    to: destination,
                }),
                ownership::Action::Copy | ownership::Action::Noop => {
                    statements.push(Statement::Assign {
                        place: destination,
                        value: Rvalue::Use(source),
                    });
                }
                ownership::Action::Retain => statements.push(Statement::Retain {
                    from: source,
                    to: destination,
                }),
                ownership::Action::Drop => unreachable!("assignment cannot use drop action"),
            }
        } else {
            if ownership::plan_for_ty(&locals[destination.local].ty).scope_exit
                == ownership::Action::Drop
            {
                statements.push(Statement::Drop(destination));
            }
            statements.push(Statement::Assign {
                place: destination,
                value,
            });
        }
        Ok(())
    }

    /// Materialize lexical ownership cleanup in MIR. Backends consume these
    /// operations instead of rediscovering scope exits from source syntax.
    /// The returned value is excluded because ownership transfers outward.
    fn append_scope_exit_drops(
        statements: &mut Vec<Statement>,
        locals: &[Local],
        first_local: usize,
        returned: Option<Place>,
    ) {
        for (local, value) in locals.iter().enumerate().skip(first_local).rev() {
            if value.name == "this" {
                continue;
            }
            if returned.is_some_and(|place| place.local == local) {
                continue;
            }
            if ownership::plan_for_ty(&value.ty).scope_exit == ownership::Action::Drop {
                statements.push(Statement::Drop(Place { local }));
            }
        }
    }

    fn append_scope_exit_drops_for_bindings(
        statements: &mut Vec<Statement>,
        locals: &[Local],
        bindings: &HashMap<String, usize>,
        returned: Option<Place>,
    ) {
        let mut active = bindings.values().copied().collect::<Vec<_>>();
        active.sort_unstable();
        for local in active.into_iter().rev() {
            if returned.is_some_and(|place| place.local == local) {
                continue;
            }
            let Some(value) = locals.get(local) else {
                continue;
            };
            if value.name == "this" {
                continue;
            }
            if ownership::plan_for_ty(&value.ty).scope_exit == ownership::Action::Drop {
                statements.push(Statement::Drop(Place { local }));
            }
        }
    }

    fn lower_await_operand(
        expr: &Expr,
        result_ty: &Ty,
        bindings: &HashMap<String, usize>,
        locals: &mut Vec<Local>,
        statements: &mut Vec<Statement>,
        checked: Option<&CheckedFile>,
    ) -> Result<Place, LowerError> {
        if let Expr::Ident(_) = expr {
            return place_for_expr(expr, bindings);
        }
        let Rvalue::Call { .. } = lower_rvalue(expr, locals, statements, bindings, checked)? else {
            return Err(LowerError::Unsupported {
                span: expr.span(),
                construct: "await operand",
            });
        };
        let local = locals.len();
        let ty = Ty::Task(Box::new(result_ty.clone()));
        locals.push(Local {
            name: format!("__await_task_{local}"),
            ty: ty.clone(),
            ownership: ownership::mode_for_ty(&ty),
        });
        let task = Place { local };
        let value = lower_rvalue(expr, locals, statements, bindings, checked)?;
        statements.push(Statement::Assign { place: task, value });
        Ok(task)
    }

    // Keep the lowering inputs explicit: branch CFG construction mutates the
    // shared block/local arenas while carrying lexical and loop scopes.
    #[allow(clippy::too_many_arguments)]
    fn lower_branch_terminal(
        branch: &Block,
        block: usize,
        join: usize,
        blocks: &mut Vec<BasicBlock>,
        locals: &mut Vec<Local>,
        bindings: &HashMap<String, usize>,
        checked: Option<&CheckedFile>,
        loop_targets: Option<(usize, usize)>,
    ) -> Result<(), LowerError> {
        let branch_local_start = locals.len();
        lower_branch_terminal_from(
            branch,
            block,
            join,
            blocks,
            locals,
            bindings,
            checked,
            loop_targets,
            branch_local_start,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_branch_terminal_from(
        branch: &Block,
        block: usize,
        join: usize,
        blocks: &mut Vec<BasicBlock>,
        locals: &mut Vec<Local>,
        bindings: &HashMap<String, usize>,
        checked: Option<&CheckedFile>,
        loop_targets: Option<(usize, usize)>,
        branch_local_start: usize,
    ) -> Result<(), LowerError> {
        let mut branch_bindings = bindings.clone();
        for (index, statement) in branch.stmts.iter().enumerate() {
            match statement {
                Stmt::Var(value) => {
                    let ty = value
                        .ty
                        .as_ref()
                        .and_then(|type_ref| {
                            checked.and_then(|file| type_ref_to_ty(type_ref, &HashMap::new(), file))
                        })
                        .or_else(|| value.ty.as_ref().and_then(crate::type_ref_builtin))
                        .or_else(|| {
                            checked.and_then(|file| {
                                file.expr_tys
                                    .get(&(value.init.span().start, value.init.span().end))
                                    .cloned()
                            })
                        })
                        .ok_or(LowerError::MissingType { span: value.span })?;
                    let local = locals.len();
                    branch_bindings.insert(value.name.name.clone(), local);
                    locals.push(Local {
                        name: format!("__mir_{block}_{}", value.name.name),
                        ty: ty.clone(),
                        ownership: ownership::mode_for_ty(&ty),
                    });
                    if let Expr::Async(AsyncExpr::Await(await_expr)) = &value.init {
                        let task = lower_await_operand(
                            &await_expr.operand,
                            &ty,
                            &branch_bindings,
                            locals,
                            &mut blocks[block].statements,
                            checked,
                        )?;
                        let resume = blocks.len();
                        blocks.push(BasicBlock {
                            statements: Vec::new(),
                            terminator: Terminator::Unreachable,
                        });
                        blocks[block].terminator = Terminator::Await {
                            task,
                            result: Place { local },
                            resume,
                            unwind: None,
                        };
                        let tail = Block {
                            stmts: branch.stmts[index + 1..].to_vec(),
                            span: branch.span,
                        };
                        return lower_branch_terminal_from(
                            &tail,
                            resume,
                            join,
                            blocks,
                            locals,
                            &branch_bindings,
                            checked,
                            loop_targets,
                            branch_local_start,
                        );
                    }
                    let rvalue = lower_rvalue(
                        &value.init,
                        locals,
                        &mut blocks[block].statements,
                        &branch_bindings,
                        checked,
                    )?;
                    blocks[block].statements.push(Statement::Assign {
                        place: Place { local },
                        value: rvalue,
                    });
                }
                Stmt::Expr(expr) => {
                    if let Expr::Assign(assign) = expr {
                        lower_assignment(
                            assign,
                            &mut blocks[block].statements,
                            &branch_bindings,
                            locals,
                            checked,
                        )?;
                        continue;
                    }
                    if let Expr::Async(AsyncExpr::Await(await_expr)) = expr {
                        let result_ty = checked
                            .and_then(|file| {
                                file.expr_tys
                                    .get(&(await_expr.span.start, await_expr.span.end))
                                    .cloned()
                            })
                            .unwrap_or(Ty::Unit);
                        let task = lower_await_operand(
                            &await_expr.operand,
                            &result_ty,
                            &branch_bindings,
                            locals,
                            &mut blocks[block].statements,
                            checked,
                        )?;
                        let result = locals.len();
                        locals.push(Local {
                            name: format!("__mir_{block}_await_discard_{result}"),
                            ty: result_ty.clone(),
                            ownership: ownership::mode_for_ty(&result_ty),
                        });
                        let resume = blocks.len();
                        blocks.push(BasicBlock {
                            statements: Vec::new(),
                            terminator: Terminator::Unreachable,
                        });
                        blocks[block].terminator = Terminator::Await {
                            task,
                            result: Place { local: result },
                            resume,
                            unwind: None,
                        };
                        let tail = Block {
                            stmts: branch.stmts[index + 1..].to_vec(),
                            span: branch.span,
                        };
                        return lower_branch_terminal_from(
                            &tail,
                            resume,
                            join,
                            blocks,
                            locals,
                            &branch_bindings,
                            checked,
                            loop_targets,
                            branch_local_start,
                        );
                    }
                    let value = lower_rvalue(
                        expr,
                        locals,
                        &mut blocks[block].statements,
                        &branch_bindings,
                        checked,
                    )?;
                    blocks[block].statements.push(Statement::Evaluate(value));
                }
                Stmt::Return(value) => {
                    let place = value
                        .value
                        .as_ref()
                        .map(|expr| {
                            place_or_temp(
                                expr,
                                locals,
                                &mut blocks[block].statements,
                                &branch_bindings,
                                checked,
                            )
                        })
                        .transpose()?;
                    append_scope_exit_drops_for_bindings(
                        &mut blocks[block].statements,
                        locals,
                        &branch_bindings,
                        place,
                    );
                    blocks[block].terminator = Terminator::Return { value: place };
                    return Ok(());
                }
                Stmt::Throw(value) => {
                    let place = place_or_temp(
                        &value.value,
                        locals,
                        &mut blocks[block].statements,
                        &branch_bindings,
                        checked,
                    )?;
                    append_scope_exit_drops_for_bindings(
                        &mut blocks[block].statements,
                        locals,
                        &branch_bindings,
                        Some(place),
                    );
                    blocks[block].terminator = Terminator::Throw {
                        value: place,
                        target: None,
                    };
                    return Ok(());
                }
                Stmt::Break(span) => {
                    let Some((break_target, _)) = loop_targets else {
                        return Err(LowerError::Unsupported {
                            span: *span,
                            construct: "break outside lowered loop",
                        });
                    };
                    append_scope_exit_drops(
                        &mut blocks[block].statements,
                        locals,
                        branch_local_start,
                        None,
                    );
                    blocks[block].terminator = Terminator::Goto {
                        target: break_target,
                    };
                    return Ok(());
                }
                Stmt::Continue(span) => {
                    let Some((_, continue_target)) = loop_targets else {
                        return Err(LowerError::Unsupported {
                            span: *span,
                            construct: "continue outside lowered loop",
                        });
                    };
                    append_scope_exit_drops(
                        &mut blocks[block].statements,
                        locals,
                        branch_local_start,
                        None,
                    );
                    blocks[block].terminator = Terminator::Goto {
                        target: continue_target,
                    };
                    return Ok(());
                }
                Stmt::If(value) => {
                    let condition = place_or_temp(
                        &value.cond,
                        locals,
                        &mut blocks[block].statements,
                        &branch_bindings,
                        checked,
                    )?;
                    let then_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let else_target = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    let nested_join = blocks.len();
                    blocks.push(BasicBlock {
                        statements: Vec::new(),
                        terminator: Terminator::Unreachable,
                    });
                    blocks[block].terminator = Terminator::SwitchInt {
                        condition,
                        then_target,
                        else_target,
                    };
                    lower_branch_terminal(
                        &value.then_block,
                        then_target,
                        nested_join,
                        blocks,
                        locals,
                        &branch_bindings,
                        checked,
                        loop_targets,
                    )?;
                    if let Some(else_block) = &value.else_block {
                        lower_branch_terminal(
                            else_block,
                            else_target,
                            nested_join,
                            blocks,
                            locals,
                            &branch_bindings,
                            checked,
                            loop_targets,
                        )?;
                    } else {
                        blocks[else_target].terminator = Terminator::Goto {
                            target: nested_join,
                        };
                    }
                    if index + 1 == branch.stmts.len() {
                        blocks[nested_join].terminator = Terminator::Goto { target: join };
                    } else {
                        let tail = Block {
                            stmts: branch.stmts[index + 1..].to_vec(),
                            span: branch.span,
                        };
                        return lower_branch_terminal_from(
                            &tail,
                            nested_join,
                            join,
                            blocks,
                            locals,
                            &branch_bindings,
                            checked,
                            loop_targets,
                            branch_local_start,
                        );
                    }
                    return Ok(());
                }
                _ => {
                    return Err(LowerError::Unsupported {
                        span: branch.span,
                        construct: "branch statement",
                    });
                }
            }
        }
        append_scope_exit_drops(
            &mut blocks[block].statements,
            locals,
            branch_local_start,
            None,
        );
        blocks[block].terminator = Terminator::Goto { target: join };
        Ok(())
    }

    fn lower_loop_body(
        body: &Block,
        block: usize,
        back_edge: usize,
        blocks: &mut Vec<BasicBlock>,
        locals: &mut Vec<Local>,
        bindings: &HashMap<String, usize>,
        checked: Option<&CheckedFile>,
    ) -> Result<(), LowerError> {
        if let Some(Stmt::Var(value)) = body.stmts.first() {
            if let Expr::Async(AsyncExpr::Await(await_expr)) = &value.init {
                let result_ty = value
                    .ty
                    .as_ref()
                    .and_then(crate::type_ref_builtin)
                    .or_else(|| {
                        checked.and_then(|file| {
                            file.expr_tys
                                .get(&(value.init.span().start, value.init.span().end))
                                .cloned()
                        })
                    })
                    .ok_or(LowerError::MissingType { span: value.span })?;
                let result_local = locals.len();
                locals.push(Local {
                    name: value.name.name.clone(),
                    ty: result_ty.clone(),
                    ownership: ownership::mode_for_ty(&result_ty),
                });
                let task = lower_await_operand(
                    &await_expr.operand,
                    &result_ty,
                    bindings,
                    locals,
                    &mut blocks[block].statements,
                    checked,
                )?;
                let resume = blocks.len();
                blocks.push(BasicBlock {
                    statements: Vec::new(),
                    terminator: Terminator::Unreachable,
                });
                blocks[block].terminator = Terminator::Await {
                    task,
                    result: Place {
                        local: result_local,
                    },
                    resume,
                    unwind: None,
                };
                let mut resumed_bindings = bindings.clone();
                resumed_bindings.insert(value.name.name.clone(), result_local);
                let tail = Block {
                    stmts: body.stmts[1..].to_vec(),
                    span: body.span,
                };
                return lower_branch_terminal(
                    &tail,
                    resume,
                    back_edge,
                    blocks,
                    locals,
                    &resumed_bindings,
                    checked,
                    Some((back_edge, back_edge)),
                );
            }
        }
        if body.stmts.len() == 1 {
            if let Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr))) = &body.stmts[0] {
                let result_ty = checked
                    .and_then(|file| {
                        file.expr_tys
                            .get(&(await_expr.span.start, await_expr.span.end))
                            .cloned()
                    })
                    .unwrap_or(Ty::Unit);
                let result_local = locals.len();
                locals.push(Local {
                    name: format!("__await_loop_result_{result_local}"),
                    ty: result_ty.clone(),
                    ownership: ownership::mode_for_ty(&result_ty),
                });
                let task = lower_await_operand(
                    &await_expr.operand,
                    &result_ty,
                    bindings,
                    locals,
                    &mut blocks[block].statements,
                    checked,
                )?;
                let resume = blocks.len();
                blocks.push(BasicBlock {
                    statements: Vec::new(),
                    terminator: Terminator::Goto { target: back_edge },
                });
                blocks[block].terminator = Terminator::Await {
                    task,
                    result: Place {
                        local: result_local,
                    },
                    resume,
                    unwind: None,
                };
                return Ok(());
            }
        }
        lower_branch_terminal(
            body,
            block,
            back_edge,
            blocks,
            locals,
            bindings,
            checked,
            Some((back_edge, back_edge)),
        )
    }

    fn stmt_span(stmt: &Stmt) -> Span {
        match stmt {
            Stmt::Var(value) => value.span,
            Stmt::If(value) => value.span,
            Stmt::While(value) => value.span,
            Stmt::ForRange(value) => value.span,
            Stmt::ForIn(value) => value.span,
            Stmt::Match(value) => value.span,
            Stmt::Try(value) => value.span,
            Stmt::Throw(value) => value.span,
            Stmt::Return(value) => value.span,
            Stmt::Expr(value) => value.span(),
            Stmt::Break(span) | Stmt::Continue(span) => *span,
        }
    }

    fn enum_field_ty(
        field: &aura_ast::TypeRef,
        declaration: &aura_ast::EnumDecl,
        scrutinee_ty: &Ty,
        checked: &CheckedFile,
    ) -> Option<Ty> {
        let substitutions = if let Ty::EnumApp { args, .. } = scrutinee_ty {
            let params = declaration
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            type_subst_map(&params, args)
        } else {
            HashMap::new()
        };
        type_ref_to_ty(field, &substitutions, checked)
    }

    pub(super) fn type_ref_to_ty(
        field: &aura_ast::TypeRef,
        substitutions: &HashMap<String, Ty>,
        checked: &CheckedFile,
    ) -> Option<Ty> {
        if field.qualifier.is_some() || field.fun.is_some() || field.reference {
            return None;
        }
        if let Some(ty) = substitutions.get(&field.name.name) {
            return Some(subst_ty(ty, substitutions));
        }
        let args = field
            .type_args
            .iter()
            .map(|arg| type_ref_to_ty(arg, substitutions, checked))
            .collect::<Option<Vec<_>>>()?;
        let ty = match field.name.name.as_str() {
            "Unit" => Ty::Unit,
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "String" => Ty::String,
            "Null" => Ty::Null,
            "Task" if args.len() == 1 => Ty::Task(Box::new(args[0].clone())),
            "TaskHandle" if args.len() == 1 => Ty::TaskHandle(Box::new(args[0].clone())),
            "Channel" if args.len() == 1 => Ty::Channel(Box::new(args[0].clone())),
            "ForeignHandle" if args.len() == 1 => Ty::ForeignHandle(Box::new(args[0].clone())),
            "Array" => Ty::ClassApp {
                name: "Array".into(),
                args,
            },
            name if checked.ast.enums.iter().any(|item| item.name.name == name) => {
                if args.is_empty() {
                    Ty::Enum(name.into())
                } else {
                    Ty::EnumApp {
                        name: name.into(),
                        args,
                    }
                }
            }
            name if checked
                .ast
                .classes
                .iter()
                .any(|item| item.name.name == name) =>
            {
                if args.is_empty() {
                    Ty::Class(name.into())
                } else {
                    Ty::ClassApp {
                        name: name.into(),
                        args,
                    }
                }
            }
            name if checked
                .ast
                .interfaces
                .iter()
                .any(|item| item.name.name == name) =>
            {
                if args.is_empty() {
                    Ty::Interface(name.into())
                } else {
                    Ty::InterfaceApp {
                        name: name.into(),
                        args,
                    }
                }
            }
            _ => return None,
        };
        Some(if field.nullable {
            Ty::Nullable(Box::new(ty))
        } else {
            ty
        })
    }

    fn iteration_action(ty: &Ty) -> ownership::Action {
        match ownership::plan_for_ty(ty).storage {
            ownership::Storage::Copy => ownership::Action::Copy,
            ownership::Storage::GcReference => ownership::Action::Retain,
            ownership::Storage::Unique
            | ownership::Storage::TaskHandle
            | ownership::Storage::Channel
            | ownership::Storage::FunctionEnvironment => ownership::Action::Clone,
        }
    }

    fn bind_loaded_value(
        statements: &mut Vec<Statement>,
        from: usize,
        to: usize,
        action: ownership::Action,
    ) {
        let from = Place { local: from };
        let to = Place { local: to };
        match action {
            ownership::Action::Move => statements.push(Statement::Move { from, to }),
            ownership::Action::Clone => statements.push(Statement::Clone { from, to }),
            ownership::Action::Retain => statements.push(Statement::Retain { from, to }),
            ownership::Action::Copy | ownership::Action::Noop => {
                statements.push(Statement::Assign {
                    place: to,
                    value: Rvalue::Use(from),
                });
            }
            ownership::Action::Drop => {}
        }
    }

    fn protocol_target(name: &str, package: &str) -> CallTarget {
        CallTarget {
            name: name.into(),
            package: package.into(),
            type_args: Vec::new(),
            method_type_args: Vec::new(),
            is_constructor: false,
            is_static: false,
            // Mark protocol dispatch so the alpha C primitive renderer does
            // not mistake a receiver call for a free function call.
            variant: Some("__iterable_protocol".into()),
        }
    }

    fn protocol_methods(
        iterable_ty: &Ty,
        checked: &CheckedFile,
        class: bool,
        span: Span,
    ) -> Result<(Ty, ProtocolLength, CallTarget), LowerError> {
        let (nominal, args) = if class {
            match iterable_ty {
                Ty::Class(name) => (name.as_str(), &[][..]),
                Ty::ClassApp { name, args } => (name.as_str(), args.as_slice()),
                _ => {
                    return Err(LowerError::Unsupported {
                        span,
                        construct: "class iterable protocol",
                    })
                }
            }
        } else {
            match iterable_ty {
                Ty::Interface(name) => (name.as_str(), &[][..]),
                Ty::InterfaceApp { name, args } => (name.as_str(), args.as_slice()),
                _ => {
                    return Err(LowerError::Unsupported {
                        span,
                        construct: "interface iterable protocol",
                    })
                }
            }
        };
        let simple = nominal.split('@').next().unwrap_or(nominal);
        if class {
            let declaration = checked
                .ast
                .classes
                .iter()
                .find(|item| item.name.name == simple)
                .ok_or(LowerError::Unsupported {
                    span,
                    construct: "unknown class iterable protocol",
                })?;
            let params = declaration
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            let substitutions = type_subst_map(&params, args);
            let len_method = declaration
                .methods
                .iter()
                .find(|method| method.name.name == "len" && method.params.is_empty());
            let len_field = declaration
                .fields
                .iter()
                .find(|field| field.name.name == "len");
            if len_method.is_none() && len_field.is_none() {
                return Err(LowerError::Unsupported {
                    span,
                    construct: "class iterable len member",
                });
            }
            let get = declaration
                .methods
                .iter()
                .find(|method| method.name.name == "get" && method.params.len() == 1)
                .ok_or(LowerError::Unsupported {
                    span,
                    construct: "class iterable get method",
                })?;
            let len_access = if let Some(len) = len_method {
                let len_ty = len
                    .return_type
                    .as_ref()
                    .and_then(|ty| type_ref_to_ty(ty, &substitutions, checked));
                if len_ty != Some(Ty::Int) {
                    return Err(LowerError::Unsupported {
                        span,
                        construct: "class iterable len type",
                    });
                }
                ProtocolLength::Method(protocol_target("len", &declaration.origin_package))
            } else if let Some(len) = len_field {
                let len_ty = type_ref_to_ty(&len.ty, &substitutions, checked);
                if len_ty != Some(Ty::Int) {
                    return Err(LowerError::Unsupported {
                        span,
                        construct: "class iterable len field type",
                    });
                }
                ProtocolLength::Field("len".into())
            } else {
                return Err(LowerError::Unsupported {
                    span,
                    construct: "class iterable len member",
                });
            };
            let element_ty = get
                .return_type
                .as_ref()
                .and_then(|ty| type_ref_to_ty(ty, &substitutions, checked))
                .ok_or(LowerError::Unsupported {
                    span,
                    construct: "class iterable element type",
                })?;
            return Ok((
                element_ty,
                len_access,
                protocol_target("get", &declaration.origin_package),
            ));
        }
        let declaration = checked
            .ast
            .interfaces
            .iter()
            .find(|item| item.name.name == simple)
            .ok_or(LowerError::Unsupported {
                span,
                construct: "unknown interface iterable protocol",
            })?;
        let params = declaration
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect::<Vec<_>>();
        let substitutions = type_subst_map(&params, args);
        let len = declaration
            .methods
            .iter()
            .find(|method| method.name.name == "len" && method.params.is_empty())
            .ok_or(LowerError::Unsupported {
                span,
                construct: "interface iterable len method",
            })?;
        let get = declaration
            .methods
            .iter()
            .find(|method| method.name.name == "get" && method.params.len() == 1)
            .ok_or(LowerError::Unsupported {
                span,
                construct: "interface iterable get method",
            })?;
        let len_ty = len
            .return_type
            .as_ref()
            .and_then(|ty| type_ref_to_ty(ty, &substitutions, checked));
        if len_ty != Some(Ty::Int) {
            return Err(LowerError::Unsupported {
                span,
                construct: "interface iterable len type",
            });
        }
        let element_ty = get
            .return_type
            .as_ref()
            .and_then(|ty| type_ref_to_ty(ty, &substitutions, checked))
            .ok_or(LowerError::Unsupported {
                span,
                construct: "interface iterable element type",
            })?;
        Ok((
            element_ty,
            ProtocolLength::Method(protocol_target("len", &declaration.origin_package)),
            protocol_target("get", &declaration.origin_package),
        ))
    }

    fn place_for_expr(expr: &Expr, locals: &HashMap<String, usize>) -> Result<Place, LowerError> {
        let name = match expr {
            Expr::Ident(ident) => &ident.name,
            Expr::This(_) => "this",
            _ => {
                return Err(LowerError::Unsupported {
                    span: expr.span(),
                    construct: "non-local await operand",
                });
            }
        };
        locals
            .get(name)
            .copied()
            .map(|local| Place { local })
            .ok_or_else(|| LowerError::UnknownLocal {
                span: expr.span(),
                name: name.to_string(),
            })
    }

    fn field_type_for_ty(ty: &Ty, field: &str, checked: Option<&CheckedFile>) -> Option<Ty> {
        let checked = checked?;
        let class_name = match ty {
            Ty::Class(name) => name,
            Ty::ClassApp { name, .. } => name,
            _ => return None,
        };
        let simple_name = class_name.split('@').next().unwrap_or(class_name);
        let class = checked
            .ast
            .classes
            .iter()
            .find(|class| class.name.name == simple_name)?;
        let declaration = class.fields.iter().find(|value| value.name.name == field)?;
        type_ref_to_ty(&declaration.ty, &HashMap::new(), checked)
    }

    fn place_or_temp(
        expr: &Expr,
        locals: &mut Vec<Local>,
        statements: &mut Vec<Statement>,
        bindings: &HashMap<String, usize>,
        checked: Option<&CheckedFile>,
    ) -> Result<Place, LowerError> {
        if let Expr::Group(inner, _) = expr {
            return place_or_temp(inner, locals, statements, bindings, checked);
        }
        if matches!(expr, Expr::Ident(_) | Expr::This(_)) {
            let name = match expr {
                Expr::Ident(ident) => &ident.name,
                Expr::This(_) => "this",
                _ => unreachable!("guarded above"),
            };
            if let Some(local) = bindings.get(name).copied() {
                return Ok(Place { local });
            }
            let Expr::Ident(identifier) = expr else {
                return Err(LowerError::UnknownLocal {
                    span: expr.span(),
                    name: name.to_string(),
                });
            };
            if checked.is_some_and(|file| {
                file.ast
                    .consts
                    .iter()
                    .any(|constant| constant.name.name == identifier.name)
            }) {
                // Constants are materialized as literal MIR values below.
            } else {
                let object =
                    bindings
                        .get("this")
                        .copied()
                        .ok_or_else(|| LowerError::UnknownLocal {
                            span: identifier.span,
                            name: identifier.name.clone(),
                        })?;
                let object_ty = locals[object].ty.clone();
                let field_ty = field_type_for_ty(&object_ty, &identifier.name, checked)
                    .ok_or_else(|| LowerError::UnknownLocal {
                        span: identifier.span,
                        name: identifier.name.clone(),
                    })?;
                let local = locals.len();
                let ownership = ownership::mode_for_ty(&field_ty);
                locals.push(Local {
                    name: format!("__field_{local}"),
                    ty: field_ty,
                    ownership,
                });
                statements.push(Statement::Assign {
                    place: Place { local },
                    value: Rvalue::Field {
                        object: Place { local: object },
                        field: identifier.name.clone(),
                    },
                });
                return Ok(Place { local });
            }
        }
        if let Some(ty) = checked.and_then(|file| {
            file.expr_tys
                .get(&(expr.span().start, expr.span().end))
                .cloned()
        }) {
            let local = locals.len();
            locals.push(Local {
                name: format!("__return_{local}"),
                ty: ty.clone(),
                ownership: ownership::mode_for_ty(&ty),
            });
            let value = lower_rvalue(expr, locals, statements, bindings, checked)?;
            statements.push(Statement::Assign {
                place: Place { local },
                value,
            });
            return Ok(Place { local });
        }
        let ty = match expr {
            Expr::Int(_) => Ty::Int,
            Expr::Bool(_) => Ty::Bool,
            Expr::String(_) => Ty::String,
            Expr::Null(_) => Ty::Null,
            Expr::Unary(value) => match value.op {
                aura_ast::UnOp::Neg => Ty::Int,
                aura_ast::UnOp::Not => Ty::Bool,
            },
            Expr::Binary(value) => match value.op {
                aura_ast::BinOp::Eq
                | aura_ast::BinOp::Ne
                | aura_ast::BinOp::Lt
                | aura_ast::BinOp::Le
                | aura_ast::BinOp::Gt
                | aura_ast::BinOp::Ge
                | aura_ast::BinOp::And
                | aura_ast::BinOp::Or => Ty::Bool,
                _ => Ty::Int,
            },
            Expr::Call(value) => checked
                .and_then(|file| {
                    file.expr_tys
                        .get(&(value.span.start, value.span.end))
                        .cloned()
                })
                .ok_or(LowerError::MissingType { span: value.span })?,
            Expr::Field(value) => checked
                .and_then(|file| {
                    file.expr_tys
                        .get(&(value.span.start, value.span.end))
                        .cloned()
                })
                .ok_or(LowerError::MissingType { span: value.span })?,
            Expr::If(value) => checked
                .and_then(|file| {
                    file.expr_tys
                        .get(&(value.span.start, value.span.end))
                        .cloned()
                })
                .ok_or(LowerError::MissingType { span: value.span })?,
            Expr::ForceUnwrap(value) => checked
                .and_then(|file| {
                    file.expr_tys
                        .get(&(value.span.start, value.span.end))
                        .cloned()
                })
                .ok_or(LowerError::MissingType { span: value.span })?,
            Expr::Is(_) => Ty::Bool,
            _ => {
                return Err(LowerError::Unsupported {
                    span: expr.span(),
                    construct: "return expression",
                })
            }
        };
        let local = locals.len();
        locals.push(Local {
            name: format!("__return_{local}"),
            ty: ty.clone(),
            ownership: ownership::mode_for_ty(&ty),
        });
        let value = lower_rvalue(expr, locals, statements, bindings, checked)?;
        statements.push(Statement::Assign {
            place: Place { local },
            value,
        });
        Ok(Place { local })
    }

    /// Normalize a direct `return await expr` into a typed local followed by
    /// return. This is frontend lowering and must not live in a backend.
    pub fn normalize_return_await(f: &AsyncFunDecl) -> Option<AsyncFunDecl> {
        let mut lowered = f.clone();
        let return_type = lowered.return_type.clone()?;
        let mut changed = false;
        let mut stmts = Vec::with_capacity(lowered.body.stmts.len() + 1);
        for stmt in lowered.body.stmts {
            match stmt {
                Stmt::Return(mut ret)
                    if matches!(ret.value, Some(Expr::Async(AsyncExpr::Await(_)))) && !changed =>
                {
                    let value = ret.value.take().expect("matched await return");
                    let name = Ident {
                        name: format!("__aura_await_return_{}", ret.span.start),
                        span: ret.span,
                    };
                    stmts.push(Stmt::Var(VarStmt {
                        mutable: false,
                        name: name.clone(),
                        ty: Some(return_type.clone()),
                        init: value,
                        span: ret.span,
                    }));
                    ret.value = Some(Expr::Ident(name));
                    stmts.push(Stmt::Return(ret));
                    changed = true;
                }
                other => stmts.push(other),
            }
        }
        if !changed {
            return None;
        }
        lowered.body.stmts = stmts;
        Some(lowered)
    }
}

pub mod ownership {
    use super::Ty;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Storage {
        Copy,
        Unique,
        GcReference,
        TaskHandle,
        Channel,
        FunctionEnvironment,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        Copy,
        Move,
        Clone,
        Retain,
        Drop,
        Noop,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Plan {
        pub storage: Storage,
        pub bind: Action,
        pub assign: Action,
        pub across_suspend: Action,
        pub scope_exit: Action,
    }

    pub fn plan_for_ty(ty: &Ty) -> Plan {
        match ty {
            Ty::Unit | Ty::Int | Ty::Float | Ty::Bool | Ty::Null => Plan {
                storage: Storage::Copy,
                bind: Action::Copy,
                assign: Action::Copy,
                across_suspend: Action::Copy,
                scope_exit: Action::Noop,
            },
            Ty::String | Ty::ForeignHandle(_) => Plan {
                storage: Storage::Unique,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Clone,
                scope_exit: Action::Drop,
            },
            Ty::Class(_) | Ty::Interface(_) | Ty::InterfaceApp { .. } | Ty::Nullable(_) => Plan {
                storage: Storage::GcReference,
                bind: Action::Retain,
                assign: Action::Retain,
                across_suspend: Action::Retain,
                scope_exit: Action::Drop,
            },
            Ty::ClassApp { name, .. } if name == "Array" => Plan {
                storage: Storage::Unique,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Clone,
                scope_exit: Action::Drop,
            },
            Ty::ClassApp { .. } | Ty::Enum(_) | Ty::EnumApp { .. } => Plan {
                storage: Storage::Unique,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Clone,
                scope_exit: Action::Drop,
            },
            Ty::Fun { .. } => Plan {
                storage: Storage::FunctionEnvironment,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Clone,
                scope_exit: Action::Drop,
            },
            Ty::Task(_) | Ty::TaskHandle(_) => Plan {
                storage: Storage::TaskHandle,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Retain,
                scope_exit: Action::Drop,
            },
            Ty::Channel(_) => Plan {
                storage: Storage::Channel,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Retain,
                scope_exit: Action::Drop,
            },
            Ty::TypeParam(_) => Plan {
                storage: Storage::Unique,
                bind: Action::Move,
                assign: Action::Move,
                across_suspend: Action::Clone,
                scope_exit: Action::Drop,
            },
        }
    }

    pub fn mode_for_ty(ty: &Ty) -> super::OwnershipMode {
        match plan_for_ty(ty).storage {
            Storage::Copy => super::OwnershipMode::Borrowed,
            Storage::Unique
            | Storage::TaskHandle
            | Storage::Channel
            | Storage::FunctionEnvironment => super::OwnershipMode::Owned,
            Storage::GcReference => super::OwnershipMode::Shared,
        }
    }
}

pub mod exceptions {
    use aura_ast::{Block, Span, Stmt};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Region {
        pub owner: String,
        pub try_span: Span,
        pub catch_span: Option<Span>,
        pub finally_span: Option<Span>,
        pub has_throw: bool,
    }

    pub fn collect(owner: &str, block: &Block) -> Vec<Region> {
        let mut regions = Vec::new();
        collect_block(owner, block, &mut regions);
        regions
    }

    fn collect_block(owner: &str, block: &Block, out: &mut Vec<Region>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Try(value) => {
                    out.push(Region {
                        owner: owner.into(),
                        try_span: value.span,
                        catch_span: value.catch.as_ref().map(|catch| catch.span),
                        finally_span: value.finally.as_ref().map(|block| block.span),
                        has_throw: contains_throw(&value.try_block),
                    });
                    collect_block(owner, &value.try_block, out);
                    if let Some(catch) = &value.catch {
                        collect_block(owner, &catch.body, out);
                    }
                    if let Some(finally) = &value.finally {
                        collect_block(owner, finally, out);
                    }
                }
                Stmt::If(value) => {
                    collect_block(owner, &value.then_block, out);
                    if let Some(block) = &value.else_block {
                        collect_block(owner, block, out);
                    }
                }
                Stmt::While(value) => collect_block(owner, &value.body, out),
                Stmt::ForRange(value) => collect_block(owner, &value.body, out),
                Stmt::ForIn(value) => collect_block(owner, &value.body, out),
                Stmt::Match(value) => {
                    for arm in &value.arms {
                        collect_block(owner, &arm.body, out);
                    }
                }
                Stmt::Var(_)
                | Stmt::Break(_)
                | Stmt::Continue(_)
                | Stmt::Throw(_)
                | Stmt::Return(_)
                | Stmt::Expr(_) => {}
            }
        }
    }

    fn contains_throw(block: &Block) -> bool {
        block.stmts.iter().any(|stmt| match stmt {
            Stmt::Throw(_) => true,
            Stmt::If(value) => {
                contains_throw(&value.then_block)
                    || value.else_block.as_ref().is_some_and(contains_throw)
            }
            Stmt::While(value) => contains_throw(&value.body),
            Stmt::ForRange(value) => contains_throw(&value.body),
            Stmt::ForIn(value) => contains_throw(&value.body),
            Stmt::Match(value) => value.arms.iter().any(|arm| contains_throw(&arm.body)),
            Stmt::Try(value) => {
                contains_throw(&value.try_block)
                    || value
                        .catch
                        .as_ref()
                        .is_some_and(|catch| contains_throw(&catch.body))
                    || value.finally.as_ref().is_some_and(contains_throw)
            }
            Stmt::Var(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_) | Stmt::Expr(_) => {
                false
            }
        })
    }
}

pub mod generics {
    use std::collections::{HashMap, HashSet};

    use aura_ast::{AsyncExpr, Block, Expr, LambdaBody, Stmt};
    use aura_sema::{subst_ty, type_subst_map, CheckedFile, Ty};

    use super::GenericInstantiation;

    pub fn collect(source: &CheckedFile) -> Vec<GenericInstantiation> {
        let mut result = Vec::new();
        for (owner, args) in &source.mono_classes {
            result.push(GenericInstantiation {
                owner: owner.clone(),
                args: args.clone(),
                kind: super::GenericOwnerKind::Class,
            });
        }
        for (owner, args) in &source.mono_enums {
            result.push(GenericInstantiation {
                owner: owner.clone(),
                args: args.clone(),
                kind: super::GenericOwnerKind::Enum,
            });
        }
        for (owner, args) in &source.mono_funs {
            result.push(GenericInstantiation {
                owner: owner.clone(),
                args: args.clone(),
                kind: super::GenericOwnerKind::Function,
            });
        }
        for (owner, args) in &source.mono_async_funs {
            result.push(GenericInstantiation {
                owner: owner.clone(),
                args: args.clone(),
                kind: super::GenericOwnerKind::AsyncFunction,
            });
        }
        for (owner, args) in &source.mono_interfaces {
            result.push(GenericInstantiation {
                owner: owner.clone(),
                args: args.clone(),
                kind: super::GenericOwnerKind::Interface,
            });
        }
        for (class, args, method, method_args) in &source.mono_methods {
            result.push(GenericInstantiation {
                owner: format!("{class}::{method}"),
                args: args.iter().chain(method_args).cloned().collect(),
                kind: super::GenericOwnerKind::Method,
            });
        }
        result.sort_by(|left, right| {
            (&left.owner, &left.kind, &ty_sort_key(&left.args)).cmp(&(
                &right.owner,
                &right.kind,
                &ty_sort_key(&right.args),
            ))
        });
        result.dedup();
        result
    }

    /// Close generic free-function calls transitively before backend selection.
    ///
    /// Sema records direct instantiations, but a generic body may call another
    /// generic function with its own type parameter. This traversal performs
    /// that substitution over the checked AST once, in the frontend IR layer;
    /// backends consume the resulting deterministic work list instead of
    /// rediscovering it from syntax.
    pub fn collect_closed_functions(source: &CheckedFile) -> Vec<GenericInstantiation> {
        let generic_names = source
            .ast
            .functions
            .iter()
            .filter(|function| !function.type_params.is_empty())
            .map(|function| function.name.name.clone())
            .collect::<HashSet<_>>();
        let mut queue = source.mono_funs.to_vec();
        let mut seen = queue.iter().cloned().collect::<HashSet<_>>();
        let mut result = queue
            .iter()
            .map(|(owner, args)| GenericInstantiation {
                owner: owner.clone(),
                args: args.clone(),
                kind: super::GenericOwnerKind::Function,
            })
            .collect::<Vec<_>>();
        let mut index = 0;
        while let Some((owner, args)) = queue.get(index).cloned() {
            index += 1;
            let Some(function) = source
                .ast
                .functions
                .iter()
                .find(|function| function.name.name == owner)
            else {
                continue;
            };
            let params = function
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            let substitutions = type_subst_map(&params, &args);
            collect_block_calls(
                &function.body,
                source,
                &substitutions,
                &generic_names,
                &mut seen,
                &mut queue,
                &mut result,
            );
        }
        result.sort_by(|left, right| {
            (&left.owner, &left.kind, ty_sort_key(&left.args)).cmp(&(
                &right.owner,
                &right.kind,
                ty_sort_key(&right.args),
            ))
        });
        result.dedup();
        result
    }

    fn collect_block_calls(
        block: &Block,
        source: &CheckedFile,
        substitutions: &HashMap<String, Ty>,
        generic_names: &HashSet<String>,
        seen: &mut HashSet<(String, Vec<Ty>)>,
        queue: &mut Vec<(String, Vec<Ty>)>,
        result: &mut Vec<GenericInstantiation>,
    ) {
        for statement in &block.stmts {
            match statement {
                Stmt::Var(value) => collect_expr_calls(
                    &value.init,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                Stmt::If(value) => {
                    collect_expr_calls(
                        &value.cond,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    collect_block_calls(
                        &value.then_block,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    if let Some(block) = &value.else_block {
                        collect_block_calls(
                            block,
                            source,
                            substitutions,
                            generic_names,
                            seen,
                            queue,
                            result,
                        );
                    }
                }
                Stmt::While(value) => {
                    collect_expr_calls(
                        &value.cond,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    collect_block_calls(
                        &value.body,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                }
                Stmt::ForRange(value) => {
                    collect_expr_calls(
                        &value.start,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    collect_expr_calls(
                        &value.end,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    collect_block_calls(
                        &value.body,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                }
                Stmt::ForIn(value) => {
                    collect_expr_calls(
                        &value.iterable,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    collect_block_calls(
                        &value.body,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                }
                Stmt::Match(value) => {
                    collect_expr_calls(
                        &value.scrutinee,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    for arm in &value.arms {
                        collect_block_calls(
                            &arm.body,
                            source,
                            substitutions,
                            generic_names,
                            seen,
                            queue,
                            result,
                        );
                    }
                }
                Stmt::Try(value) => {
                    collect_block_calls(
                        &value.try_block,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    if let Some(catch) = &value.catch {
                        collect_block_calls(
                            &catch.body,
                            source,
                            substitutions,
                            generic_names,
                            seen,
                            queue,
                            result,
                        );
                    }
                    if let Some(finally) = &value.finally {
                        collect_block_calls(
                            finally,
                            source,
                            substitutions,
                            generic_names,
                            seen,
                            queue,
                            result,
                        );
                    }
                }
                Stmt::Throw(value) => collect_expr_calls(
                    &value.value,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                Stmt::Return(value) => {
                    if let Some(value) = &value.value {
                        collect_expr_calls(
                            value,
                            source,
                            substitutions,
                            generic_names,
                            seen,
                            queue,
                            result,
                        );
                    }
                }
                Stmt::Expr(value) => collect_expr_calls(
                    value,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn collect_expr_calls(
        expr: &Expr,
        source: &CheckedFile,
        substitutions: &HashMap<String, Ty>,
        generic_names: &HashSet<String>,
        seen: &mut HashSet<(String, Vec<Ty>)>,
        queue: &mut Vec<(String, Vec<Ty>)>,
        result: &mut Vec<GenericInstantiation>,
    ) {
        match expr {
            Expr::Call(call) => {
                if let Some(instantiation) = source.call_instantiations.get(&call.span.start) {
                    let args = instantiation
                        .type_args
                        .iter()
                        .map(|ty| subst_ty(ty, substitutions))
                        .collect::<Vec<_>>();
                    if generic_names.contains(&instantiation.name)
                        && args.iter().all(|ty| !ty.is_open())
                    {
                        let key = (instantiation.name.clone(), args.clone());
                        if seen.insert(key.clone()) {
                            queue.push(key);
                            result.push(GenericInstantiation {
                                owner: instantiation.name.clone(),
                                args,
                                kind: super::GenericOwnerKind::Function,
                            });
                        }
                    }
                }
                collect_expr_calls(
                    &call.callee,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                );
                for arg in &call.args {
                    collect_expr_calls(
                        arg,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                }
            }
            Expr::Field(value) => collect_expr_calls(
                &value.object,
                source,
                substitutions,
                generic_names,
                seen,
                queue,
                result,
            ),
            Expr::Assign(value) => collect_expr_calls(
                &value.value,
                source,
                substitutions,
                generic_names,
                seen,
                queue,
                result,
            ),
            Expr::Binary(value) => {
                collect_expr_calls(
                    &value.left,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                );
                collect_expr_calls(
                    &value.right,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                );
            }
            Expr::Unary(value) => collect_expr_calls(
                &value.expr,
                source,
                substitutions,
                generic_names,
                seen,
                queue,
                result,
            ),
            Expr::ForceUnwrap(value) => collect_expr_calls(
                &value.expr,
                source,
                substitutions,
                generic_names,
                seen,
                queue,
                result,
            ),
            Expr::Is(value) => collect_expr_calls(
                &value.expr,
                source,
                substitutions,
                generic_names,
                seen,
                queue,
                result,
            ),
            Expr::Group(value, _) => collect_expr_calls(
                value,
                source,
                substitutions,
                generic_names,
                seen,
                queue,
                result,
            ),
            Expr::If(value) => {
                collect_expr_calls(
                    &value.cond,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                );
                collect_block_calls(
                    &value.then_block,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                );
                collect_block_calls(
                    &value.else_block,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                );
            }
            Expr::Lambda(value) => match &value.body {
                LambdaBody::Expr(body) => collect_expr_calls(
                    body,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                LambdaBody::Block(body) => collect_block_calls(
                    body,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
            },
            Expr::Async(value) => match value {
                AsyncExpr::Await(value) => collect_expr_calls(
                    &value.operand,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                AsyncExpr::Spawn(value) => collect_block_calls(
                    &value.body,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                AsyncExpr::Join(value) => collect_expr_calls(
                    &value.handle,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                AsyncExpr::Cancel(value) => collect_expr_calls(
                    &value.handle,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                AsyncExpr::ChannelCreate(value) => collect_expr_calls(
                    &value.capacity,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                AsyncExpr::ChannelSend(value) => {
                    collect_expr_calls(
                        &value.channel,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                    collect_expr_calls(
                        &value.value,
                        source,
                        substitutions,
                        generic_names,
                        seen,
                        queue,
                        result,
                    );
                }
                AsyncExpr::ChannelReceive(value) => collect_expr_calls(
                    &value.channel,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
                AsyncExpr::ChannelClose(value) => collect_expr_calls(
                    &value.channel,
                    source,
                    substitutions,
                    generic_names,
                    seen,
                    queue,
                    result,
                ),
            },
            Expr::Ident(_)
            | Expr::This(_)
            | Expr::Int(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Float(_)
            | Expr::Null(_) => {}
        }
    }

    fn ty_sort_key(args: &[Ty]) -> String {
        args.iter().map(Ty::display).collect::<Vec<_>>().join(",")
    }
}

/// Typed async control-flow model shared by all backends.
///
/// The action payload is still an opaque lowering operation during the alpha
/// migration. It is intentionally kept here, outside aura-codegen, so the
/// next slice can replace C fragments with typed MIR statements without
/// changing backend dispatch.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Pure,
    Async,
    Throws,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipMode {
    Borrowed,
    Owned,
    Move,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFact {
    pub ty: Ty,
    pub ownership: OwnershipMode,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionIr {
    pub name: String,
    pub package: String,
    pub params: Vec<ValueFact>,
    pub ret: ValueFact,
    pub type_params: Vec<String>,
    pub bounds: HashMap<String, Vec<String>>,
    pub effect: Effect,
    /// MIR for bodies covered by the current frontend lowering subset.
    /// `None` is explicit and keeps the alpha backend fallback observable.
    pub body: Option<mir::MirBody>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenericOwnerKind {
    Class,
    Enum,
    Function,
    AsyncFunction,
    Interface,
    Method,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericInstantiation {
    pub owner: String,
    pub args: Vec<Ty>,
    pub kind: GenericOwnerKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipFact {
    pub owner: String,
    pub value: String,
    pub ty: Ty,
    pub plan: ownership::Plan,
}

/// Link metadata required by a native backend. Keeping this in CheckedIr
/// prevents backends from reaching back into the AST just to assemble linker
/// arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignLinkIr {
    Dynamic,
    Static,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignLibraryIr {
    pub function: String,
    pub library: String,
    pub link: ForeignLinkIr,
}

#[derive(Debug, Clone)]
pub struct CheckedIr {
    pub package: String,
    pub functions: Vec<FunctionIr>,
    /// Closed generic free-function instances lowered before backend choice.
    pub generic_functions: Vec<FunctionIr>,
    /// Closed generic free-function instances still outside the MIR subset.
    pub generic_function_mir_unlowered: Vec<String>,
    pub generic_instantiations: Vec<GenericInstantiation>,
    pub call_instantiations: HashMap<u32, CallInstantiation>,
    pub expr_types: HashMap<(u32, u32), Ty>,
    pub attributes: Vec<AttributeMetadata>,
    pub expansions: Vec<ExpansionMetadata>,
    pub async_functions: Vec<String>,
    pub throwing_functions: Vec<String>,
    pub ownership: Vec<OwnershipFact>,
    pub exception_regions: Vec<exceptions::Region>,
    pub async_mir: Vec<mir::MirBody>,
    /// Open generic async declarations lowered with symbolic type parameters.
    pub open_generic_async_mir: Vec<mir::MirBody>,
    /// Closed generic async free-function instances lowered before backend choice.
    pub generic_async_mir: Vec<mir::MirBody>,
    /// Closed generic async free-function instances still outside the MIR subset.
    pub generic_async_mir_unlowered: Vec<String>,
    /// Closed generic async class-method instances lowered before backend choice.
    pub generic_async_method_mir: Vec<mir::MirBody>,
    /// Closed generic synchronous class-method instances lowered before backend choice.
    pub generic_method_mir: Vec<mir::MirBody>,
    pub generic_async_state_machines: Vec<state_machine::StateMachine>,
    pub generic_async_method_state_machines: Vec<state_machine::StateMachine>,
    pub async_state_machines: Vec<state_machine::StateMachine>,
    pub open_generic_async_state_machines: Vec<state_machine::StateMachine>,
    /// State machines for nested `spawn` bodies, published recursively.
    pub spawn_state_machines: Vec<state_machine::StateMachine>,
    /// Async bodies still using the alpha compatibility lowering. This is
    /// explicit so a backend cannot mistake partial MIR coverage for success.
    pub async_mir_unlowered: Vec<String>,
    /// Open generic async declarations still outside the current MIR subset.
    pub open_generic_async_mir_unlowered: Vec<String>,
    /// Synchronous bodies still outside the current common MIR subset.
    pub function_mir_unlowered: Vec<String>,
    /// Closed generic async methods still outside the current MIR subset.
    pub generic_async_method_mir_unlowered: Vec<String>,
    /// Closed generic synchronous methods still outside the current MIR subset.
    pub generic_method_mir_unlowered: Vec<String>,
    pub foreign_libraries: Vec<ForeignLibraryIr>,
}

#[derive(Debug, Clone)]
pub struct LoweredProgram {
    pub ir: CheckedIr,
    /// Transitional input for the alpha C backend only.
    pub(crate) source: CheckedFile,
}

impl LoweredProgram {
    pub fn from_checked(source: CheckedFile) -> Self {
        let async_functions = source
            .ast
            .async_functions
            .iter()
            .map(|fun| fun.name.name.clone())
            .collect::<Vec<_>>();
        let mut throwing_functions = Vec::new();
        for fun in &source.ast.functions {
            if block_throws(&fun.body) {
                throwing_functions.push(fun.name.name.clone());
            }
        }
        for fun in &source.ast.async_functions {
            if block_throws(&fun.body) {
                throwing_functions.push(fun.name.name.clone());
            }
        }
        let effect_for = |name: &str| {
            if async_functions.iter().any(|candidate| candidate == name) {
                Effect::Async
            } else if throwing_functions.iter().any(|candidate| candidate == name) {
                Effect::Throws
            } else {
                Effect::Pure
            }
        };
        let mut functions: Vec<FunctionIr> = source
            .functions
            .iter()
            .map(|fun| {
                let effect = effect_for(&fun.name);
                let body = source
                    .ast
                    .functions
                    .iter()
                    .find(|decl| decl.name.name == fun.name)
                    .and_then(|decl| {
                        let params = decl
                            .params
                            .iter()
                            .zip(fun.params.iter())
                            .map(|(param, ty)| (param.name.name.clone(), ty.clone()))
                            .collect::<Vec<_>>();
                        lowering::lower_body(
                            &fun.name,
                            &decl.body,
                            &params,
                            fun.ret.clone(),
                            Some(&source),
                            effect,
                        )
                        .ok()
                    });
                FunctionIr {
                    name: fun.name.clone(),
                    package: fun.package.clone(),
                    params: fun
                        .params
                        .iter()
                        .map(|ty| ValueFact {
                            ty: ty.clone(),
                            ownership: ownership_of(ty),
                            span: fun.span,
                        })
                        .collect(),
                    ret: ValueFact {
                        ty: fun.ret.clone(),
                        ownership: ownership_of(&fun.ret),
                        span: fun.span,
                    },
                    type_params: fun.type_params.clone(),
                    bounds: fun.bounds.clone(),
                    effect,
                    body,
                    span: fun.span,
                }
            })
            .collect();
        for class in &source.ast.classes {
            let package = if class.origin_package.is_empty() {
                source.package.clone()
            } else {
                class.origin_package.clone()
            };
            let receiver_ty = Ty::Class(class.name.name.clone());
            for method in &class.methods {
                if !method.type_params.is_empty()
                    || method
                        .return_type
                        .as_ref()
                        .is_some_and(|ty| ty.name.name == "Task")
                {
                    continue;
                }
                let substitutions = HashMap::new();
                let mut params = vec![("this".into(), receiver_ty.clone())];
                let Some(method_params) = method
                    .params
                    .iter()
                    .map(|param| {
                        lowering::type_ref_to_ty(&param.ty, &substitutions, &source)
                            .map(|ty| (param.name.name.clone(), ty))
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                params.extend(method_params);
                let ret = method
                    .return_type
                    .as_ref()
                    .and_then(|ty| lowering::type_ref_to_ty(ty, &substitutions, &source))
                    .unwrap_or(Ty::Unit);
                let name = format!("{}::{}", class.name.name, method.name.name);
                let body = lowering::lower_body(
                    &name,
                    &method.body,
                    &params,
                    ret.clone(),
                    Some(&source),
                    if block_throws(&method.body) {
                        Effect::Throws
                    } else {
                        Effect::Pure
                    },
                )
                .ok();
                functions.push(FunctionIr {
                    name,
                    package: package.clone(),
                    params: params
                        .iter()
                        .map(|(_, ty)| ValueFact {
                            ty: ty.clone(),
                            ownership: ownership_of(ty),
                            span: method.name.span,
                        })
                        .collect(),
                    ret: ValueFact {
                        ty: ret.clone(),
                        ownership: ownership_of(&ret),
                        span: method.name.span,
                    },
                    type_params: Vec::new(),
                    bounds: HashMap::new(),
                    effect: if block_throws(&method.body) {
                        Effect::Throws
                    } else {
                        Effect::Pure
                    },
                    body,
                    span: method.span,
                });
            }
        }
        let function_mir_unlowered = functions
            .iter()
            .filter(|function| {
                function.body.is_none()
                    && !source
                        .ast
                        .foreign_functions
                        .iter()
                        .any(|foreign| foreign.name.name == function.name)
            })
            .map(|function| function.name.clone())
            .collect::<Vec<_>>();

        let mut generic_instantiations = generics::collect(&source);
        generic_instantiations.extend(generics::collect_closed_functions(&source));
        generic_instantiations.sort_by(|left, right| {
            (
                &left.owner,
                &left.kind,
                left.args.iter().map(Ty::display).collect::<Vec<_>>(),
            )
                .cmp(&(
                    &right.owner,
                    &right.kind,
                    right.args.iter().map(Ty::display).collect::<Vec<_>>(),
                ))
        });
        generic_instantiations.dedup();
        let generic_functions = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::Function)
            .filter_map(|instance| {
                let decl = source
                    .ast
                    .functions
                    .iter()
                    .find(|function| function.name.name == instance.owner)?;
                let closed = generic_lowering::close_function(decl, &instance.args, &source);
                let params = closed
                    .params
                    .iter()
                    .map(|param| Some((param.name.name.clone(), type_ref_builtin(&param.ty)?)))
                    .collect::<Option<Vec<_>>>()?;
                let ret = closed
                    .return_type
                    .as_ref()
                    .map(type_ref_builtin)
                    .unwrap_or(Some(Ty::Unit))?;
                let ret_ownership = ownership_of(&ret);
                let body = lowering::lower_body(
                    &format!(
                        "{}_{}",
                        instance.owner,
                        instance
                            .args
                            .iter()
                            .map(Ty::mono_suffix)
                            .collect::<Vec<_>>()
                            .join("_")
                    ),
                    &closed.body,
                    &params,
                    ret.clone(),
                    Some(&source),
                    Effect::Pure,
                )
                .ok();
                Some(FunctionIr {
                    name: format!(
                        "{}_{}",
                        instance.owner,
                        instance
                            .args
                            .iter()
                            .map(Ty::mono_suffix)
                            .collect::<Vec<_>>()
                            .join("_")
                    ),
                    package: source.package.clone(),
                    params: params
                        .iter()
                        .map(|(_, ty)| ValueFact {
                            ty: ty.clone(),
                            ownership: ownership_of(ty),
                            span: decl.name.span,
                        })
                        .collect(),
                    ret: ValueFact {
                        ty: ret,
                        ownership: ret_ownership,
                        span: decl.name.span,
                    },
                    type_params: Vec::new(),
                    bounds: HashMap::new(),
                    effect: Effect::Pure,
                    body,
                    span: decl.name.span,
                })
            })
            .collect::<Vec<_>>();
        let generic_function_mir_unlowered = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::Function)
            .filter_map(|instance| {
                let name = format!(
                    "{}_{}",
                    instance.owner,
                    instance
                        .args
                        .iter()
                        .map(Ty::mono_suffix)
                        .collect::<Vec<_>>()
                        .join("_")
                );
                (!generic_functions
                    .iter()
                    .any(|function| function.name == name))
                .then_some(name)
            })
            .collect::<Vec<_>>();
        let generic_async_mir = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::AsyncFunction)
            .filter_map(|instance| {
                let decl = source
                    .ast
                    .async_functions
                    .iter()
                    .find(|function| function.name.name == instance.owner)?;
                let closed = generic_lowering::close_async_function(decl, &instance.args, &source);
                let params = closed
                    .params
                    .iter()
                    .map(|param| Some((param.name.name.clone(), type_ref_builtin(&param.ty)?)))
                    .collect::<Option<Vec<_>>>()?;
                let ret = closed
                    .return_type
                    .as_ref()
                    .map(type_ref_builtin)
                    .unwrap_or(Some(Ty::Unit))?;
                lowering::lower_async_body(
                    &closed.name.name,
                    &closed.body,
                    &params,
                    ret,
                    Some(&source),
                )
                .ok()
            })
            .collect::<Vec<_>>();
        let generic_async_mir_unlowered = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::AsyncFunction)
            .filter_map(|instance| {
                let name = format!(
                    "{}_{}",
                    instance.owner,
                    instance
                        .args
                        .iter()
                        .map(Ty::mono_suffix)
                        .collect::<Vec<_>>()
                        .join("_")
                );
                (!generic_async_mir.iter().any(|body| body.name == name)).then_some(name)
            })
            .collect::<Vec<_>>();
        let generic_async_method_mir = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::Method)
            .filter_map(|instance| {
                let (class_owner, method_name) = instance.owner.split_once("::")?;
                let class_simple = class_owner.split('@').next().unwrap_or(class_owner);
                let class = source
                    .ast
                    .classes
                    .iter()
                    .find(|class| class.name.name == class_simple)?;
                let method = class
                    .methods
                    .iter()
                    .find(|method| method.name.name == method_name)?;
                let class_count = class.type_params.len();
                let method_count = method.type_params.len();
                if instance.args.len() != class_count + method_count {
                    return None;
                }
                let class_args = &instance.args[..class_count];
                let method_args = &instance.args[class_count..];
                let suffix = instance
                    .args
                    .iter()
                    .map(Ty::mono_suffix)
                    .collect::<Vec<_>>()
                    .join("_");
                let closed = generic_lowering::close_async_method(
                    &class.name,
                    method,
                    class.origin_package.clone(),
                    format!(
                        "{}_{}_{}",
                        class_owner.replace('@', "_"),
                        method_name,
                        suffix
                    ),
                    &class
                        .type_params
                        .iter()
                        .map(|param| param.name.name.clone())
                        .collect::<Vec<_>>(),
                    class_args,
                    method_args,
                )?;
                let empty_substitutions = HashMap::new();
                let params = closed
                    .params
                    .iter()
                    .map(|param| {
                        lowering::type_ref_to_ty(&param.ty, &empty_substitutions, &source)
                            .map(|ty| (param.name.name.clone(), ty))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let ret = closed
                    .return_type
                    .as_ref()
                    .and_then(|ty| lowering::type_ref_to_ty(ty, &empty_substitutions, &source))
                    .unwrap_or(Ty::Unit);
                lowering::lower_async_body(
                    &closed.name.name,
                    &closed.body,
                    &params,
                    ret,
                    Some(&source),
                )
                .ok()
            })
            .collect::<Vec<_>>();
        let generic_async_method_mir_unlowered = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::Method)
            .filter_map(|instance| {
                let (class_owner, method_name) = instance.owner.split_once("::")?;
                let class_simple = class_owner.split('@').next().unwrap_or(class_owner);
                let class = source
                    .ast
                    .classes
                    .iter()
                    .find(|class| class.name.name == class_simple)?;
                let method = class
                    .methods
                    .iter()
                    .find(|method| method.name.name == method_name)?;
                let is_async = method
                    .return_type
                    .as_ref()
                    .is_some_and(|ty| ty.name.name == "Task");
                if !is_async {
                    return None;
                }
                let prefix = format!("{}_{}", class_owner.replace('@', "_"), method_name);
                (!generic_async_method_mir
                    .iter()
                    .any(|body| body.name.starts_with(&prefix)))
                .then_some(instance.owner.clone())
            })
            .collect::<Vec<_>>();
        let generic_method_mir = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::Method)
            .filter_map(|instance| {
                let (class_owner, method_name) = instance.owner.split_once("::")?;
                let class_simple = class_owner.split('@').next().unwrap_or(class_owner);
                let class = source
                    .ast
                    .classes
                    .iter()
                    .find(|class| class.name.name == class_simple)?;
                let method = class
                    .methods
                    .iter()
                    .find(|method| method.name.name == method_name)?;
                if method
                    .return_type
                    .as_ref()
                    .is_some_and(|ty| ty.name.name == "Task")
                {
                    return None;
                }
                let class_count = class.type_params.len();
                let method_count = method.type_params.len();
                if instance.args.len() != class_count + method_count {
                    return None;
                }
                let class_args = &instance.args[..class_count];
                let method_args = &instance.args[class_count..];
                let suffix = instance
                    .args
                    .iter()
                    .map(Ty::mono_suffix)
                    .collect::<Vec<_>>()
                    .join("_");
                let closed = generic_lowering::close_method(
                    &class.name,
                    method,
                    class.origin_package.clone(),
                    format!(
                        "{}_{}_{}",
                        class_owner.replace('@', "_"),
                        method_name,
                        suffix
                    ),
                    &class
                        .type_params
                        .iter()
                        .map(|param| param.name.name.clone())
                        .collect::<Vec<_>>(),
                    class_args,
                    method_args,
                );
                let empty_substitutions = HashMap::new();
                let params = closed
                    .params
                    .iter()
                    .map(|param| {
                        lowering::type_ref_to_ty(&param.ty, &empty_substitutions, &source)
                            .map(|ty| (param.name.name.clone(), ty))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let ret = closed
                    .return_type
                    .as_ref()
                    .and_then(|ty| lowering::type_ref_to_ty(ty, &empty_substitutions, &source))
                    .unwrap_or(Ty::Unit);
                lowering::lower_body(
                    &closed.name.name,
                    &closed.body,
                    &params,
                    ret,
                    Some(&source),
                    Effect::Pure,
                )
                .ok()
            })
            .collect::<Vec<_>>();
        let generic_method_mir_unlowered = generic_instantiations
            .iter()
            .filter(|instance| instance.kind == GenericOwnerKind::Method)
            .filter_map(|instance| {
                let (class_owner, method_name) = instance.owner.split_once("::")?;
                let class_simple = class_owner.split('@').next().unwrap_or(class_owner);
                let class = source
                    .ast
                    .classes
                    .iter()
                    .find(|class| class.name.name == class_simple)?;
                let method = class
                    .methods
                    .iter()
                    .find(|method| method.name.name == method_name)?;
                if method
                    .return_type
                    .as_ref()
                    .is_some_and(|ty| ty.name.name == "Task")
                {
                    return None;
                }
                let prefix = format!("{}_{}", class_owner.replace('@', "_"), method_name);
                (!generic_method_mir
                    .iter()
                    .any(|body| body.name.starts_with(&prefix)))
                .then_some(instance.owner.clone())
            })
            .collect::<Vec<_>>();
        let mut ownership = Vec::new();
        for function in &source.functions {
            for (index, ty) in function.params.iter().enumerate() {
                ownership.push(OwnershipFact {
                    owner: function.name.clone(),
                    value: format!("param_{index}"),
                    ty: ty.clone(),
                    plan: ownership::plan_for_ty(ty),
                });
            }
            ownership.push(OwnershipFact {
                owner: function.name.clone(),
                value: "return".into(),
                ty: function.ret.clone(),
                plan: ownership::plan_for_ty(&function.ret),
            });
        }
        let exception_regions =
            source
                .ast
                .functions
                .iter()
                .flat_map(|function| exceptions::collect(&function.name.name, &function.body))
                .chain(
                    source.ast.async_functions.iter().flat_map(|function| {
                        exceptions::collect(&function.name.name, &function.body)
                    }),
                )
                .collect();
        let mut async_mir = Vec::new();
        let mut async_mir_unlowered = Vec::new();
        let mut open_generic_async_mir = Vec::new();
        let mut open_generic_async_mir_unlowered = Vec::new();
        for function in &source.ast.async_functions {
            if !function.type_params.is_empty() {
                let symbolic = function
                    .type_params
                    .iter()
                    .map(|param| {
                        (
                            param.name.name.clone(),
                            Ty::TypeParam(param.name.name.clone()),
                        )
                    })
                    .collect::<HashMap<_, _>>();
                let Some(return_ty) = function
                    .return_type
                    .as_ref()
                    .and_then(|ty| lowering::type_ref_to_ty(ty, &symbolic, &source))
                else {
                    open_generic_async_mir_unlowered.push(function.name.name.clone());
                    continue;
                };
                let Some(params) = function
                    .params
                    .iter()
                    .map(|param| {
                        lowering::type_ref_to_ty(&param.ty, &symbolic, &source)
                            .map(|ty| (param.name.name.clone(), ty))
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    open_generic_async_mir_unlowered.push(function.name.name.clone());
                    continue;
                };
                match lowering::lower_async_body(
                    &function.name.name,
                    &function.body,
                    &params,
                    return_ty,
                    Some(&source),
                ) {
                    Ok(body) => open_generic_async_mir.push(body),
                    Err(_) => open_generic_async_mir_unlowered.push(function.name.name.clone()),
                }
                continue;
            }
            let Some(return_ty) = function
                .return_type
                .as_ref()
                .map(type_ref_builtin)
                .unwrap_or(Some(Ty::Unit))
            else {
                async_mir_unlowered.push(function.name.name.clone());
                continue;
            };
            let Some(params) = function
                .params
                .iter()
                .map(|param| Some((param.name.name.clone(), type_ref_builtin(&param.ty)?)))
                .collect::<Option<Vec<_>>>()
            else {
                async_mir_unlowered.push(function.name.name.clone());
                continue;
            };
            match lowering::lower_async_body(
                &function.name.name,
                &function.body,
                &params,
                return_ty,
                Some(&source),
            ) {
                Ok(body) => async_mir.push(body),
                Err(_) => async_mir_unlowered.push(function.name.name.clone()),
            }
        }
        for body in &async_mir {
            for local in &body.locals {
                ownership.push(OwnershipFact {
                    owner: body.name.clone(),
                    value: local.name.clone(),
                    ty: local.ty.clone(),
                    plan: ownership::plan_for_ty(&local.ty),
                });
            }
        }
        let async_state_machines = async_mir
            .iter()
            .filter_map(|body| state_machine::StateMachine::from_mir(body).ok())
            .collect();
        let open_generic_async_state_machines = open_generic_async_mir
            .iter()
            .filter_map(|body| state_machine::StateMachine::from_mir(body).ok())
            .collect();
        let generic_async_state_machines = generic_async_mir
            .iter()
            .filter_map(|body| state_machine::StateMachine::from_mir(body).ok())
            .collect();
        let generic_async_method_state_machines = generic_async_method_mir
            .iter()
            .filter_map(|body| state_machine::StateMachine::from_mir(body).ok())
            .collect();
        let mut spawn_state_machines = Vec::new();
        for body in functions
            .iter()
            .filter_map(|function| function.body.as_ref())
            .chain(
                generic_functions
                    .iter()
                    .filter_map(|function| function.body.as_ref()),
            )
            .chain(async_mir.iter())
            .chain(open_generic_async_mir.iter())
            .chain(generic_async_mir.iter())
            .chain(generic_async_method_mir.iter())
            .chain(generic_method_mir.iter())
        {
            collect_spawn_state_machines(body, &mut spawn_state_machines);
        }

        let ir = CheckedIr {
            package: source.package.clone(),
            functions,
            generic_functions,
            generic_function_mir_unlowered,
            generic_instantiations,
            call_instantiations: source.call_instantiations.clone(),
            expr_types: source.expr_tys.clone(),
            attributes: source.attribute_metadata.clone(),
            expansions: source.expansions.clone(),
            async_functions,
            throwing_functions,
            ownership,
            exception_regions,
            async_mir,
            open_generic_async_mir,
            generic_async_mir,
            generic_async_mir_unlowered,
            generic_async_method_mir,
            generic_method_mir,
            generic_async_state_machines,
            generic_async_method_state_machines,
            async_state_machines,
            open_generic_async_state_machines,
            spawn_state_machines,
            async_mir_unlowered,
            open_generic_async_mir_unlowered,
            function_mir_unlowered,
            generic_async_method_mir_unlowered,
            generic_method_mir_unlowered,
            foreign_libraries: source
                .ast
                .foreign_functions
                .iter()
                .filter_map(|foreign| {
                    let library = foreign.library.as_ref()?;
                    let link = foreign.link.as_ref()?;
                    Some(ForeignLibraryIr {
                        function: foreign.name.name.clone(),
                        library: library.name.clone(),
                        link: match link.kind {
                            ForeignLinkKind::Dynamic => ForeignLinkIr::Dynamic,
                            ForeignLinkKind::Static => ForeignLinkIr::Static,
                        },
                    })
                })
                .collect(),
        };
        Self { ir, source }
    }

    pub fn checked(&self) -> &CheckedIr {
        &self.ir
    }

    /// Exposes checked declarations needed by backends for nominal layout.
    pub fn source(&self) -> &CheckedFile {
        &self.source
    }

    pub fn async_mir(&self) -> &[mir::MirBody] {
        &self.ir.async_mir
    }

    pub fn async_state_machines(&self) -> &[state_machine::StateMachine] {
        &self.ir.async_state_machines
    }

    pub fn unlowered_async_names(&self) -> &[String] {
        &self.ir.async_mir_unlowered
    }

    pub fn unlowered_mir_names(&self) -> Vec<String> {
        self.ir
            .async_mir_unlowered
            .iter()
            .chain(self.ir.open_generic_async_mir_unlowered.iter())
            .chain(self.ir.function_mir_unlowered.iter())
            .chain(self.ir.generic_function_mir_unlowered.iter())
            .chain(self.ir.generic_async_mir_unlowered.iter())
            .chain(self.ir.generic_async_method_mir_unlowered.iter())
            .chain(self.ir.generic_method_mir_unlowered.iter())
            .cloned()
            .collect()
    }

    pub fn mir_is_complete(&self) -> bool {
        self.ir.async_mir_unlowered.is_empty()
            && self.ir.open_generic_async_mir_unlowered.is_empty()
            && self.ir.function_mir_unlowered.is_empty()
            && self.ir.generic_function_mir_unlowered.is_empty()
            && self.ir.generic_async_mir_unlowered.is_empty()
            && self.ir.generic_async_method_mir_unlowered.is_empty()
            && self.ir.generic_method_mir_unlowered.is_empty()
    }

    /// Alpha C compatibility input. Target-neutral backends must not use this.
    pub fn alpha_c_source(&self) -> &CheckedFile {
        &self.source
    }

    pub fn foreign_libraries(&self) -> &[ForeignLibraryIr] {
        &self.ir.foreign_libraries
    }
}

fn type_ref_builtin(ty: &TypeRef) -> Option<Ty> {
    if ty.qualifier.is_some() || !ty.type_args.is_empty() || ty.fun.is_some() || ty.reference {
        return None;
    }
    let base = match ty.name.name.as_str() {
        "Unit" => Ty::Unit,
        "Int" => Ty::Int,
        "Bool" => Ty::Bool,
        "String" => Ty::String,
        _ => return None,
    };
    Some(if ty.nullable {
        Ty::Nullable(Box::new(base))
    } else {
        base
    })
}

fn block_throws(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Throw(_) => true,
        Stmt::If(value) => {
            block_throws(&value.then_block) || value.else_block.as_ref().is_some_and(block_throws)
        }
        Stmt::While(value) => block_throws(&value.body),
        Stmt::ForRange(value) => block_throws(&value.body),
        Stmt::ForIn(value) => block_throws(&value.body),
        Stmt::Match(value) => value.arms.iter().any(|arm| block_throws(&arm.body)),
        Stmt::Try(value) => {
            block_throws(&value.try_block)
                || value
                    .catch
                    .as_ref()
                    .is_some_and(|catch| block_throws(&catch.body))
                || value.finally.as_ref().is_some_and(block_throws)
        }
        Stmt::Var(_) | Stmt::Return(_) | Stmt::Expr(_) | Stmt::Break(_) | Stmt::Continue(_) => {
            false
        }
    })
}

fn ownership_of(ty: &Ty) -> OwnershipMode {
    ownership::mode_for_ty(ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_ast::{Block, Expr, IntLit, ReturnStmt, Span};
    use mir::{BasicBlock, MirBody, Place, Terminator};

    #[test]
    fn ownership_facts_are_not_target_language_types() {
        let fact = ValueFact {
            ty: Ty::String,
            ownership: OwnershipMode::Owned,
            span: Span::new(1, 2),
        };
        assert_eq!(fact.ownership, OwnershipMode::Owned);
        assert_eq!(fact.ty, Ty::String);
    }

    #[test]
    fn mir_validation_rejects_backend_unsafe_edges() {
        let body = MirBody {
            name: "demo".into(),
            locals: vec![],
            blocks: vec![BasicBlock {
                statements: vec![],
                terminator: Terminator::Return {
                    value: Some(Place { local: 0 }),
                },
            }],
            entry: 0,
            return_ty: Ty::Unit,
            effect: Effect::Pure,
        };
        assert!(matches!(
            body.validate(),
            Err(mir::ValidationError::InvalidLocal { block: 0, local: 0 })
        ));
    }

    #[test]
    fn async_state_machine_validation_and_dump_are_backend_independent() {
        let body = MirBody {
            name: "resume".into(),
            locals: vec![
                mir::Local {
                    name: "task".into(),
                    ty: Ty::Task(Box::new(Ty::Int)),
                    ownership: OwnershipMode::Owned,
                },
                mir::Local {
                    name: "result".into(),
                    ty: Ty::Int,
                    ownership: OwnershipMode::Borrowed,
                },
            ],
            blocks: vec![
                BasicBlock {
                    statements: vec![],
                    terminator: Terminator::Await {
                        task: Place { local: 0 },
                        result: Place { local: 1 },
                        resume: 1,
                        unwind: None,
                    },
                },
                BasicBlock {
                    statements: vec![],
                    terminator: Terminator::Return {
                        value: Some(Place { local: 1 }),
                    },
                },
            ],
            entry: 0,
            return_ty: Ty::Int,
            effect: Effect::Async,
        };
        let machine = state_machine::StateMachine::from_mir(&body).expect("valid MIR");
        assert_eq!(machine.entry, 0);
        assert_eq!(machine.states[0].successors, vec![1]);
        assert_eq!(machine.frame_locals, vec![0, 1]);
        assert_eq!(machine.states[1].successors, Vec::<usize>::new());
    }

    #[test]
    fn linear_async_lowering_produces_typed_mir_without_c_fragments() {
        let span = Span::new(0, 1);
        let source = Block {
            stmts: vec![Stmt::Return(ReturnStmt {
                value: Some(Expr::Int(IntLit { value: 7, span })),
                span,
            })],
            span,
        };
        let body = lowering::lower_async_body("answer", &source, &[], Ty::Int, None)
            .expect("literal return lowers to MIR");
        assert_eq!(body.locals.len(), 1);
        assert!(matches!(
            body.blocks[0].statements[0],
            mir::Statement::Assign {
                value: mir::Rvalue::ConstInt(7),
                ..
            }
        ));
        assert!(matches!(
            body.blocks[0].terminator,
            mir::Terminator::Return { value: Some(_) }
        ));
    }

    #[test]
    fn local_assignment_lowers_into_mir_without_source_syntax() {
        let file = aura_parser::parse_file(
            "package demo\nfun bump(): Int { var value: Int = 1 value = value + 2 return value }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "bump")
            .and_then(|function| function.body.as_ref())
            .expect("bump MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Binary {
                            op: mir::BinaryOp::Add,
                            ..
                        },
                        ..
                    }
                )
            })
        }));
        assert!(body.validate().is_ok());
        assert!(!program
            .checked()
            .function_mir_unlowered
            .iter()
            .any(|name| name == "bump"));
    }

    #[test]
    fn gc_collect_lowers_to_backend_neutral_intrinsic() {
        let file =
            aura_parser::parse_file("package demo\nfun collect(): Unit { gc_collect() return }\n")
                .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "collect")
            .and_then(|function| function.body.as_ref())
            .expect("collect MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Evaluate(mir::Rvalue::Intrinsic(mir::Intrinsic::GcCollect))
                )
            })
        }));
    }

    #[test]
    fn owned_local_assignment_materializes_replacement_drop_and_move() {
        let file = aura_parser::parse_file(
            "package demo\nfun replace(): String { var value: String = \"old\" val next: String = \"new\" value = next return value }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "replace")
            .and_then(|function| function.body.as_ref())
            .expect("replace MIR");
        let value = body
            .locals
            .iter()
            .position(|local| local.name == "value")
            .expect("value local");
        let next = body
            .locals
            .iter()
            .position(|local| local.name == "next")
            .expect("next local");
        assert!(body.blocks.iter().any(|block| {
            block.statements.windows(2).any(|statements| {
                matches!(statements[0], mir::Statement::Drop(Place { local }) if local == value)
                    && matches!(
                        statements[1],
                        mir::Statement::Move {
                            from: Place { local: source },
                            to: Place { local: destination },
                        } if source == next && destination == value
                    )
            })
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn assignment_inside_branch_lowers_without_c_fallback() {
        let file = aura_parser::parse_file(
            "package demo\nfun choose(flag: Bool): Int { var value: Int = 1 if (flag) { value = value + 1 } return value }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .and_then(|function| function.body.as_ref())
            .expect("choose MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Binary {
                            op: mir::BinaryOp::Add,
                            ..
                        },
                        ..
                    }
                )
            })
        }));
        assert!(body.validate().is_ok());
        assert!(!program
            .checked()
            .function_mir_unlowered
            .iter()
            .any(|name| name == "choose"));
    }

    #[test]
    fn break_and_continue_lower_to_explicit_loop_edges() {
        let file = aura_parser::parse_file(
            "package demo\nfun loops(flag: Bool): Unit { while (flag) { break } while (flag) { continue } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "loops")
            .and_then(|function| function.body.as_ref())
            .expect("loops MIR");
        let edges = body
            .blocks
            .iter()
            .filter_map(|block| match block.terminator {
                mir::Terminator::Goto { target } => Some(target),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(edges.len() >= 4);
        assert!(body.validate().is_ok());
        assert!(!program
            .checked()
            .function_mir_unlowered
            .iter()
            .any(|name| name == "loops"));
    }

    #[test]
    fn loop_exit_cleanup_does_not_drop_outer_locals() {
        let file = aura_parser::parse_file(
            "package demo\nfun keep(flag: Bool): String { val value: String = \"keep\" while (flag) { break } return value }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "keep")
            .and_then(|function| function.body.as_ref())
            .expect("keep MIR");
        let value = body
            .locals
            .iter()
            .position(|local| local.name == "value")
            .expect("value local");
        assert!(body.blocks.iter().all(|block| {
            !block
                .statements
                .iter()
                .any(|statement| matches!(statement, mir::Statement::Drop(Place { local }) if *local == value))
                || matches!(block.terminator, mir::Terminator::Return { .. })
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn unit_enum_match_lowers_to_backend_neutral_tag_switch() {
        let file = aura_parser::parse_file(
            "package demo\nenum Color { case Red case Green }\nfun classify(color: Color): Int { match (color) { case Red => { return 1 } case Green => { return 2 } } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "classify")
            .and_then(|function| function.body.as_ref())
            .expect("classify MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::VariantTag { .. },
                        ..
                    }
                )
            })
        }));
        assert!(body.blocks.iter().any(|block| {
            matches!(block.terminator, mir::Terminator::SwitchTag { ref targets, .. } if targets.len() == 2)
        }));
        assert!(body.validate().is_ok());
        assert!(!program
            .checked()
            .function_mir_unlowered
            .iter()
            .any(|name| name == "classify"));
        assert!(state_machine::StateMachine::from_mir(body).is_ok());
    }

    #[test]
    fn primitive_enum_match_binding_lowers_to_typed_variant_field() {
        let file = aura_parser::parse_file(
            "package demo\nenum Choice { case Some(value: Int) case None }\nfun unwrap(choice: Choice): Int { match (choice) { case Some(value) => { return value } case None => { return 0 } } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .and_then(|function| function.body.as_ref())
            .expect("unwrap MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::ExtractVariantField {
                        variant,
                        field,
                        action: ownership::Action::Copy,
                        ..
                    } if variant == "Some" && field == "value"
                )
            })
        }));
        assert!(body.validate().is_ok());
        assert!(state_machine::StateMachine::from_mir(body).is_ok());
    }

    #[test]
    fn generic_owned_enum_match_substitutes_payload_and_preserves_move_action() {
        let file = aura_parser::parse_file(
            "package demo\nenum Result<T, E> { case Ok(value: T) case Err(error: E) }\nfun unwrap(result: Result<String, Int>): String { match (result) { case Ok(value) => { return value } case Err(error) => { return \"fallback\" } } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .and_then(|function| function.body.as_ref())
            .expect("unwrap MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::ExtractVariantField {
                        variant,
                        field,
                        action: ownership::Action::Move,
                        ..
                    } if variant == "Ok" && field == "value"
                )
            })
        }));
        assert!(body.validate().is_ok());
        assert!(state_machine::StateMachine::from_mir(body).is_ok());
    }

    #[test]
    fn nullable_enum_match_binding_preserves_shared_payload_action() {
        let file = aura_parser::parse_file(
            "package demo\nenum MaybeText { case Some(value: String?) case None }\nfun inspect(value: MaybeText): Unit { match (value) { case Some(text) => { return } case None => { return } } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "inspect")
            .and_then(|function| function.body.as_ref())
            .expect("inspect MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::ExtractVariantField {
                        variant,
                        field,
                        action: ownership::Action::Retain,
                        ..
                    } if variant == "Some" && field == "value"
                )
            })
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn array_enum_match_binding_lowers_with_owned_extraction() {
        let file = aura_parser::parse_file(
            "package demo\nenum MaybeArray { case Some(value: Array<Int>) case None }\nfun inspect(value: MaybeArray): Unit { match (value) { case Some(items) => { return } case None => { return } } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "inspect")
            .and_then(|function| function.body.as_ref())
            .expect("inspect MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::ExtractVariantField {
                        variant,
                        field,
                        action: ownership::Action::Move,
                        ..
                    } if variant == "Some" && field == "value"
                )
            })
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn array_for_in_lowers_to_typed_index_loop_cfg() {
        let file = aura_parser::parse_file(
            "package demo\nfun sum(values: Array<Int>): Int { var total: Int = 0 for (value in values) { total = total + value } return total }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "sum")
            .and_then(|function| function.body.as_ref())
            .expect("sum MIR");
        assert!(body.blocks.iter().any(|block| {
            block
                .statements
                .iter()
                .any(|statement| matches!(statement, mir::Statement::LoadIndex { .. }))
        }));
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Length(_),
                        ..
                    }
                )
            })
        }));
        assert!(body
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, mir::Terminator::SwitchInt { .. })));
        assert!(body.validate().is_ok());
        assert!(!program
            .checked()
            .function_mir_unlowered
            .iter()
            .any(|name| name == "sum"));
    }

    #[test]
    fn string_for_in_lowers_bytes_to_int_index_loop_cfg() {
        let file = aura_parser::parse_file(
            "package demo\nfun count(value: String): Int { var total: Int = 0 for (byte in value) { total = total + byte } return total }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "count")
            .and_then(|function| function.body.as_ref())
            .expect("count MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::LoadIndex {
                        action: ownership::Action::Copy,
                        ..
                    }
                )
            })
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn protocol_for_in_lowers_len_and_get_as_neutral_method_calls() {
        let file = aura_parser::parse_file(
            "package demo\ninterface Iterable { fun len(): Int fun get(i: Int): Int }\nclass Range(val n: Int) : Iterable { fun len(): Int { return this.n } fun get(i: Int): Int { return i } }\nclass FieldRange(val len: Int) { fun get(i: Int): Int { return i } }\nfun sum(it: Iterable): Int { var total: Int = 0 for (x in it) { total = total + x } return total }\nfun sumRange(range: Range): Int { for (x in range) { return x } return 0 }\nfun sumField(range: FieldRange): Int { for (x in range) { return x } return 0 }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let sum_decl = checked
            .ast
            .functions
            .iter()
            .find(|function| function.name.name == "sum")
            .expect("sum declaration");
        lowering::lower_body(
            "sum",
            &sum_decl.body,
            &[("it".into(), Ty::Interface("Iterable".into()))],
            Ty::Int,
            Some(&checked),
            Effect::Pure,
        )
        .expect("protocol lowering");
        let program = LoweredProgram::from_checked(checked);
        for name in ["sum", "sumRange"] {
            let body = program
                .checked()
                .functions
                .iter()
                .find(|function| function.name == name)
                .and_then(|function| function.body.as_ref())
                .unwrap_or_else(|| {
                    panic!(
                        "protocol MIR missing for {name}; unlowered={:?}",
                        program.checked().function_mir_unlowered
                    )
                });
            assert!(body.blocks.iter().any(|block| {
                block.statements.iter().any(|statement| {
                    matches!(
                        statement,
                        mir::Statement::Assign {
                            value: mir::Rvalue::Call { target, .. },
                            ..
                        } if target.name == "len"
                    )
                })
            }));
            assert!(body.blocks.iter().any(|block| {
                block.statements.iter().any(|statement| {
                    matches!(
                        statement,
                        mir::Statement::Assign {
                            value: mir::Rvalue::Call { target, .. },
                            ..
                        } if target.name == "get"
                    )
                })
            }));
            assert!(body.validate().is_ok());
        }
        let field_body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "sumField")
            .and_then(|function| function.body.as_ref())
            .expect("field protocol MIR");
        assert!(field_body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Field { field, .. },
                        ..
                    } if field == "len"
                )
            })
        }));
        assert!(field_body.validate().is_ok());
        assert!(program.checked().function_mir_unlowered.is_empty());
    }

    #[test]
    fn expression_lowering_keeps_unary_and_binary_ops_out_of_backend_syntax() {
        let span = Span::new(0, 3);
        let source = Block {
            stmts: vec![Stmt::Return(ReturnStmt {
                value: Some(Expr::Binary(aura_ast::BinaryExpr {
                    op: aura_ast::BinOp::Add,
                    left: Box::new(Expr::Unary(aura_ast::UnaryExpr {
                        op: aura_ast::UnOp::Neg,
                        expr: Box::new(Expr::Int(IntLit { value: 2, span })),
                        span,
                    })),
                    right: Box::new(Expr::Int(IntLit { value: 3, span })),
                    span,
                })),
                span,
            })],
            span,
        };
        let body = lowering::lower_async_body("arithmetic", &source, &[], Ty::Int, None)
            .expect("arithmetic expression lowers to MIR");
        assert!(body.blocks[0].statements.iter().any(|statement| matches!(
            statement,
            mir::Statement::Assign {
                value: mir::Rvalue::Unary {
                    op: mir::UnaryOp::Neg,
                    ..
                },
                ..
            }
        )));
        assert!(body.blocks[0].statements.iter().any(|statement| matches!(
            statement,
            mir::Statement::Assign {
                value: mir::Rvalue::Binary {
                    op: mir::BinaryOp::Add,
                    ..
                },
                ..
            }
        )));
    }

    #[test]
    fn pure_expression_if_lowers_to_backend_neutral_select() {
        let file = aura_parser::parse_file(
            "package demo\nfun choose(flag: Bool): Int { return (if (flag) { 1 } else { 2 }) }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .and_then(|function| function.body.as_ref())
            .expect("choose MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Select { .. },
                        ..
                    }
                )
            })
        }));
        assert!(body.validate().is_ok());
        assert!(program.mir_is_complete());
    }

    #[test]
    fn top_level_constants_materialize_in_control_flow_mir() {
        let file = aura_parser::parse_file(
            "package demo\nconst LIMIT: Int = 3\nfun check(): Bool { if (LIMIT == 3) { return true } return false }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "check")
            .and_then(|function| function.body.as_ref())
            .expect("check MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::ConstInt(3),
                        ..
                    }
                )
            })
        }));
        assert!(program.mir_is_complete());
    }

    #[test]
    fn unwrap_and_type_test_lower_to_typed_neutral_operations() {
        let file = aura_parser::parse_file(
            "package demo\nclass Text() {}\nfun unwrap(value: String?): String { return value!! }\nfun isText(value: Text?): Bool { return value is Text }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let unwrap_body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .and_then(|function| function.body.as_ref())
            .expect("unwrap MIR");
        let type_test_body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "isText")
            .and_then(|function| function.body.as_ref())
            .expect("type-test MIR");
        assert!(unwrap_body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Unwrap { .. },
                        ..
                    }
                )
            })
        }));
        assert!(type_test_body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::TypeTest {
                            ty: Ty::Class(_),
                            ..
                        },
                        ..
                    }
                )
            })
        }));
        assert!(program.mir_is_complete());
    }

    #[test]
    fn side_effecting_call_statement_is_preserved_in_mir() {
        let span = Span::new(0, 1);
        let source = Block {
            stmts: vec![
                Stmt::Expr(Expr::Call(aura_ast::CallExpr {
                    callee: Box::new(Expr::Ident(aura_ast::Ident {
                        name: "touch".into(),
                        span,
                    })),
                    type_args: Vec::new(),
                    args: Vec::new(),
                    span,
                })),
                Stmt::Return(ReturnStmt { value: None, span }),
            ],
            span,
        };
        let body = lowering::lower_body("caller", &source, &[], Ty::Unit, None, Effect::Pure)
            .expect("call statement lowers to MIR");
        assert!(matches!(
            body.blocks[0].statements.first(),
            Some(mir::Statement::Evaluate(mir::Rvalue::Call { target, args }))
                if target.name == "touch" && target.package.is_empty() && args.is_empty()
        ));
        assert_eq!(body.effect, Effect::Pure);
    }

    #[test]
    fn ownership_plan_makes_suspend_action_explicit() {
        let plan = ownership::plan_for_ty(&Ty::String);
        assert_eq!(plan.bind, ownership::Action::Move);
        assert_eq!(plan.across_suspend, ownership::Action::Clone);
        assert_eq!(plan.scope_exit, ownership::Action::Drop);
    }

    #[test]
    fn scope_exit_cleanup_is_materialized_in_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun discard() { val value: String = \"owned\" }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find_map(|function| function.body.as_ref())
            .expect("MIR body");
        assert!(body.blocks[0]
            .statements
            .iter()
            .any(|statement| matches!(statement, mir::Statement::Drop(_))));
    }

    #[test]
    fn returned_owned_local_is_not_dropped_before_return() {
        let file = aura_parser::parse_file(
            "package demo\nfun keep(value: String): String { return value }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find_map(|function| function.body.as_ref())
            .expect("MIR body");
        assert!(!body.blocks[0]
            .statements
            .iter()
            .any(|statement| matches!(statement, mir::Statement::Drop(_))));
    }

    #[test]
    fn generic_async_method_closure_is_backend_neutral() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box<T>(val item: T) { fun get(value: T): Task<T> { return value } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let class = checked.ast.classes.first().expect("class");
        let method = class.methods.first().expect("method");
        let closed = generic_lowering::close_async_method(
            &class.name,
            method,
            "demo".into(),
            "Box_get_String".into(),
            &["T".into()],
            &[Ty::String],
            &[],
        )
        .expect("closed method");
        assert_eq!(closed.params[1].ty.name.name, "String");
        assert_eq!(closed.return_type.expect("return").name.name, "String");
        assert!(closed.type_params.is_empty());
    }

    #[test]
    fn generic_async_method_instance_publishes_mir_and_state_machine() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box<T>(val item: T) { fun get(value: T): Task<T> { return value } }\nfun use(box: Box<String>): Task<String> { return box.get(\"value\") }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        assert!(program
            .checked()
            .generic_instantiations
            .iter()
            .any(|instance| instance.kind == GenericOwnerKind::Method));
        let body = program
            .checked()
            .generic_async_method_mir
            .first()
            .expect("generic method MIR");
        assert!(matches!(body.locals.first().map(|local| &local.ty),
            Some(Ty::ClassApp { name, args }) if name == "Box" && args == &vec![Ty::String]));
        assert!(body
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, mir::Terminator::Return { .. })));
        assert!(body.validate().is_ok());
        assert!(program
            .checked()
            .generic_async_method_state_machines
            .iter()
            .any(|machine| machine.function == body.name));
        assert!(program
            .checked()
            .generic_async_method_mir_unlowered
            .is_empty());
    }

    #[test]
    fn generic_sync_method_instance_publishes_mir_before_backend_selection() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box<T>(val item: T) { fun get(): T { return this.item } }\nfun use(box: Box<String>): String { return box.get() }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let class = checked.ast.classes.first().expect("class");
        let method = class.methods.first().expect("method");
        let closed = generic_lowering::close_method(
            &class.name,
            method,
            "demo".into(),
            "Box_get_String".into(),
            &["T".into()],
            &[Ty::String],
            &[],
        );
        let empty_substitutions = HashMap::new();
        let params = closed
            .params
            .iter()
            .map(|param| {
                lowering::type_ref_to_ty(&param.ty, &empty_substitutions, &checked)
                    .map(|ty| (param.name.name.clone(), ty))
            })
            .collect::<Option<Vec<_>>>()
            .expect("closed parameter types");
        let ret = closed
            .return_type
            .as_ref()
            .and_then(|ty| lowering::type_ref_to_ty(ty, &empty_substitutions, &checked))
            .expect("closed return type");
        lowering::lower_body(
            "Box_get_String",
            &closed.body,
            &params,
            ret,
            Some(&checked),
            Effect::Pure,
        )
        .expect("generic method lowering");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .generic_method_mir
            .first()
            .expect("generic sync method MIR");
        assert!(matches!(body.locals.first().map(|local| &local.ty),
            Some(Ty::ClassApp { name, args }) if name == "Box" && args == &vec![Ty::String]));
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Field { field, .. },
                        ..
                    } if field == "item"
                )
            })
        }));
        assert!(body.validate().is_ok());
        assert!(program.checked().functions.iter().any(|function| {
            function.name == "use"
                && function.body.as_ref().is_some_and(|body| {
                    body.blocks.iter().any(|block| {
                        block.statements.iter().any(|statement| {
                            matches!(
                                statement,
                                mir::Statement::Assign {
                                    value: mir::Rvalue::Call { target, args },
                                    ..
                                } if target.name == "get" && args.len() == 1
                            )
                        })
                    })
                })
        }));
        assert!(program.checked().generic_method_mir_unlowered.is_empty());
        assert!(program.mir_is_complete());
    }

    #[test]
    fn conditional_async_lowering_builds_backend_neutral_cfg() {
        let span = Span::new(0, 1);
        let source = Block {
            stmts: vec![Stmt::If(aura_ast::IfStmt {
                cond: Expr::Ident(aura_ast::Ident {
                    name: "cond".into(),
                    span,
                }),
                then_block: Block {
                    stmts: vec![Stmt::Return(ReturnStmt {
                        value: Some(Expr::Int(IntLit { value: 1, span })),
                        span,
                    })],
                    span,
                },
                else_block: Some(Block {
                    stmts: vec![Stmt::Return(ReturnStmt {
                        value: Some(Expr::Int(IntLit { value: 0, span })),
                        span,
                    })],
                    span,
                }),
                span,
            })],
            span,
        };
        let body = lowering::lower_async_body(
            "choose",
            &source,
            &[("cond".into(), Ty::Bool)],
            Ty::Int,
            None,
        )
        .expect("conditional lowers to MIR");
        assert!(matches!(
            body.blocks[0].terminator,
            mir::Terminator::SwitchInt { .. }
        ));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn empty_while_lowering_builds_loop_back_edge() {
        let span = Span::new(0, 1);
        let source = Block {
            stmts: vec![Stmt::While(aura_ast::WhileStmt {
                cond: Expr::Ident(aura_ast::Ident {
                    name: "keep".into(),
                    span,
                }),
                body: Block {
                    stmts: Vec::new(),
                    span,
                },
                span,
            })],
            span,
        };
        let body = lowering::lower_async_body(
            "loop",
            &source,
            &[("keep".into(), Ty::Bool)],
            Ty::Unit,
            None,
        )
        .expect("empty loop lowers to MIR");
        assert!(body.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                mir::Terminator::Goto { target } if target < body.blocks.len()
            )
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn checked_program_materializes_supported_async_body_as_mir() {
        let file = aura_parser::parse_file("package demo\nasync fun answer(): Int { return 7 }\n")
            .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        assert_eq!(program.checked().async_mir.len(), 1);
        assert!(program.checked().async_mir_unlowered.is_empty());
        assert_eq!(program.checked().async_mir[0].name, "answer");
        assert_eq!(program.checked().async_mir[0].return_ty, Ty::Int);
        assert_eq!(program.async_state_machines().len(), 1);
        assert_eq!(program.async_state_machines()[0].entry, 0);
        assert!(program.async_state_machines()[0].states[0]
            .suspension
            .is_none());
    }

    #[test]
    fn checked_program_materializes_supported_sync_body_with_pure_effect() {
        let file = aura_parser::parse_file("package demo\nfun answer(): Int { return 7 }\n")
            .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let function = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "answer")
            .expect("function IR");
        assert_eq!(function.effect, Effect::Pure);
        assert!(function.body.is_some());
        assert!(program.checked().function_mir_unlowered.is_empty());
    }

    #[test]
    fn checked_async_mir_records_owned_local_move() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun moveIt(s: String): String { val x: String = s return x }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        assert_eq!(program.checked().async_mir.len(), 1);
        assert!(program.checked().async_mir[0].blocks.iter().any(|block| {
            block
                .statements
                .iter()
                .any(|statement| matches!(statement, mir::Statement::Move { .. }))
        }));
    }

    #[test]
    fn checked_async_mir_lowers_call_await_into_task_and_resume_edges() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun child(): Int { return 1 }\nasync fun parent(): Int { val x: Int = await child() return x }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let parent = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "parent")
            .expect("parent MIR");
        assert!(parent
            .blocks
            .iter()
            .any(|block| { matches!(block.terminator, mir::Terminator::Await { .. }) }));
        assert!(parent.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Call { target, .. },
                        ..
                    } if target.name == "child"
                )
            })
        }));
        let machine = program
            .async_state_machines()
            .iter()
            .find(|machine| machine.function == "parent")
            .expect("parent state machine");
        assert!(machine
            .states
            .iter()
            .any(|state| state.suspension.is_some()));
        assert_eq!(
            machine.frame_locals,
            (0..parent.locals.len()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn throwing_literal_lowers_value_and_cleanup_into_neutral_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun fail(): Unit { val message: String = \"boom\" throw message }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "fail")
            .and_then(|function| function.body.as_ref())
            .expect("fail MIR");
        let throw_block = body
            .blocks
            .iter()
            .find(|block| matches!(block.terminator, mir::Terminator::Throw { .. }))
            .expect("throw block");
        assert!(throw_block.statements.iter().any(|statement| {
            matches!(
                statement,
                mir::Statement::Assign {
                    value: mir::Rvalue::ConstString(value),
                    ..
                } if value == "boom"
            )
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn await_in_conditional_branch_lowers_to_backend_neutral_continuation() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun child(): Int { return 1 }\nasync fun parent(flag: Bool): Int { if (flag) { val value: Int = await child() return value } return 0 }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "parent")
            .expect("parent MIR");
        let await_resume = body
            .blocks
            .iter()
            .find_map(|block| match block.terminator {
                mir::Terminator::Await { resume, .. } => Some(resume),
                _ => None,
            })
            .expect("branch await");
        assert!(matches!(
            body.blocks[await_resume].terminator,
            mir::Terminator::Return { .. } | mir::Terminator::Goto { .. }
        ));
        assert!(body.validate().is_ok());
        assert!(program.checked().async_mir_unlowered.is_empty());
    }

    #[test]
    fn nested_conditional_body_continues_into_backend_neutral_join() {
        let file = aura_parser::parse_file(
            "package demo\nfun nested(flag: Bool): Int { if (flag) { if (flag) { } val value: Int = 1 return value } return 0 }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "nested")
            .and_then(|function| function.body.as_ref())
            .expect("nested MIR");
        assert!(body
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, mir::Terminator::SwitchInt { .. })));
        assert!(body.validate().is_ok());
        assert!(program.checked().function_mir_unlowered.is_empty());
    }

    #[test]
    fn async_state_machine_publishes_suspend_ownership_actions() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun child(): String { return \"child\" }\nasync fun parent(s: String): String { val x: String = await child() return x }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let machine = program
            .checked()
            .async_state_machines
            .iter()
            .find(|machine| machine.function == "parent")
            .expect("parent state machine");
        let suspension = machine
            .states
            .iter()
            .find_map(|state| state.suspension.as_ref())
            .expect("suspension");
        assert!(suspension
            .ownership
            .iter()
            .any(|transfer| { transfer.action == ownership::Action::Clone }));
    }

    #[test]
    fn return_await_normalization_is_backend_independent() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun child(): Int { return 1 }\nasync fun parent(): Int { return await child() }\n",
        )
        .expect("parse");
        let parent = file
            .async_functions
            .iter()
            .find(|function| function.name.name == "parent")
            .expect("parent");
        let normalized = lowering::normalize_return_await(parent).expect("normalized");
        assert!(matches!(normalized.body.stmts.first(), Some(Stmt::Var(_))));
        assert!(matches!(
            normalized.body.stmts.get(1),
            Some(Stmt::Return(_))
        ));
    }

    #[test]
    fn while_body_lowers_to_backend_neutral_loop_cfg() {
        let file = aura_parser::parse_file(
            "package demo\nfun touch() { }\nfun spin(flag: Bool) { while (flag) { touch() } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "spin")
            .and_then(|function| function.body.as_ref())
            .expect("spin MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Evaluate(mir::Rvalue::Call { target, .. })
                        if target.name == "touch"
                )
            })
        }));
        assert!(body
            .blocks
            .iter()
            .any(|block| { matches!(block.terminator, mir::Terminator::SwitchInt { .. }) }));
    }

    #[test]
    fn for_range_lowers_to_typed_counter_cfg() {
        let file = aura_parser::parse_file(
            "package demo\nfun touch(value: Int) { }\nfun count() { for (i in 0..3) { touch(i) } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "count")
            .and_then(|function| function.body.as_ref())
            .expect("count MIR");
        assert!(body.locals.iter().any(|local| local.name == "i"));
        assert!(body
            .blocks
            .iter()
            .any(|block| { matches!(block.terminator, mir::Terminator::SwitchInt { .. }) }));
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(statement, mir::Statement::Evaluate(mir::Rvalue::Call { target, .. }) if target.name == "touch")
            })
        }));
    }

    #[test]
    fn async_while_await_lowers_to_resumable_loop_cfg() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun tick(): Unit { }\nasync fun spin(flag: Bool): Unit { while (flag) { await tick() } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "spin")
            .expect("spin async MIR");
        assert!(body
            .blocks
            .iter()
            .any(|block| { matches!(block.terminator, mir::Terminator::Await { .. }) }));
        let machine = program
            .checked()
            .async_state_machines
            .iter()
            .find(|machine| machine.function == "spin")
            .expect("spin state machine");
        let suspension = machine
            .states
            .iter()
            .find_map(|state| state.suspension.as_ref())
            .expect("loop suspension");
        assert!(machine.states[suspension.resume]
            .successors
            .iter()
            .any(|target| *target < suspension.resume));
    }

    #[test]
    fn async_discarded_awaits_lower_to_separate_resume_edges() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun tick(): Unit { }\nasync fun run(): Unit { await tick() await tick() return }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "run")
            .expect("run async MIR");
        assert_eq!(
            body.blocks
                .iter()
                .filter(|block| matches!(block.terminator, mir::Terminator::Await { .. }))
                .count(),
            2
        );
        assert!(program.checked().async_mir_unlowered.is_empty());
    }

    #[test]
    fn task_handle_operations_lower_to_neutral_async_ops() {
        let file = aura_parser::parse_file(
            "package demo\nfun run(handle: TaskHandle<Int>): Unit { join(handle) cancel(handle) return }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "run")
            .and_then(|function| function.body.as_ref())
            .expect("run MIR");
        assert_eq!(
            body.blocks
                .iter()
                .flat_map(|block| block.statements.iter())
                .filter(|statement| matches!(
                    statement,
                    mir::Statement::Evaluate(mir::Rvalue::AsyncOp(_))
                ))
                .count(),
            2
        );
    }

    #[test]
    fn channel_creation_lowers_to_async_op_and_methods_remain_calls() {
        let file = aura_parser::parse_file(
            "package demo\nfun run(): Unit { val channel: Channel<Int> = Channel<Int>(1) channel.send(7) channel.receive() channel.close() return }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "run")
            .and_then(|function| function.body.as_ref())
            .expect("run MIR");
        let async_op_count = body
            .blocks
            .iter()
            .flat_map(|block| block.statements.iter())
            .filter(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::AsyncOp(_),
                        ..
                    } | mir::Statement::Evaluate(mir::Rvalue::AsyncOp(_))
                )
            })
            .count();
        assert_eq!(async_op_count, 1);

        let method_calls = body
            .blocks
            .iter()
            .flat_map(|block| block.statements.iter())
            .filter_map(|statement| match statement {
                mir::Statement::Assign {
                    value: mir::Rvalue::Call { target, .. },
                    ..
                }
                | mir::Statement::Evaluate(mir::Rvalue::Call { target, .. }) => {
                    Some(target.name.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for method in ["send", "receive", "close"] {
            assert!(
                method_calls.contains(&method),
                "expected channel method call `{method}`, got {method_calls:?}"
            );
        }
    }

    #[test]
    fn capture_free_spawn_lowers_nested_body_to_neutral_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun run(): Unit { val task = spawn { return } join(task) return }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "run")
            .and_then(|function| function.body.as_ref())
            .expect("run MIR");
        let Some(mir::Rvalue::AsyncOp(mir::AsyncOp::Spawn { body, captures })) = body
            .blocks
            .iter()
            .flat_map(|block| block.statements.iter())
            .find_map(|statement| match statement {
                mir::Statement::Assign { value, .. } => Some(value),
                _ => None,
            })
        else {
            panic!("spawn MIR");
        };
        assert!(captures.is_empty());
        assert!(body.validate().is_ok());
        assert_eq!(program.checked().spawn_state_machines.len(), 1);
    }

    #[test]
    fn captured_spawn_lowers_capture_actions_and_nested_parameters() {
        let file = aura_parser::parse_file(
            "package demo\nfun run(value: Int): Unit { val task = spawn { return value } join(task) return }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "run")
            .and_then(|function| function.body.as_ref())
            .expect("run MIR");
        let Some(mir::Rvalue::AsyncOp(mir::AsyncOp::Spawn { body, captures })) = body
            .blocks
            .iter()
            .flat_map(|block| block.statements.iter())
            .find_map(|statement| match statement {
                mir::Statement::Assign { value, .. } => Some(value),
                _ => None,
            })
        else {
            panic!("spawn MIR");
        };
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].source.local, 0);
        assert_eq!(captures[0].action, ownership::Action::Copy);
        assert_eq!(body.locals[0].name, "value");
        assert!(body.validate().is_ok());
        assert_eq!(program.checked().spawn_state_machines.len(), 1);
    }

    #[test]
    fn async_while_await_value_lowers_resumed_body_statements() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun tick(): Int { return 1 }\nfun touch(value: Int) { }\nasync fun spin(flag: Bool): Unit { while (flag) { val value: Int = await tick() touch(value) } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "spin")
            .expect("spin async MIR");
        let resume = body
            .blocks
            .iter()
            .find_map(|block| match block.terminator {
                mir::Terminator::Await { resume, .. } => Some(resume),
                _ => None,
            })
            .expect("await resume");
        assert!(body.blocks[resume].statements.iter().any(|statement| {
            matches!(statement, mir::Statement::Evaluate(mir::Rvalue::Call { target, .. }) if target.name == "touch")
        }));
    }

    #[test]
    fn async_for_range_await_preserves_counter_resume_edge() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun tick(): Int { return 1 }\nfun touch(value: Int) { }\nasync fun count(): Unit { for (i in 0..3) { val value: Int = await tick() touch(value) } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "count")
            .expect("count async MIR");
        let resume = body
            .blocks
            .iter()
            .find_map(|block| match block.terminator {
                mir::Terminator::Await { resume, .. } => Some(resume),
                _ => None,
            })
            .expect("await resume");
        assert!(matches!(
            body.blocks[resume].terminator,
            mir::Terminator::Goto { .. }
        ));
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::Assign {
                        value: mir::Rvalue::Binary {
                            op: mir::BinaryOp::Add,
                            ..
                        },
                        ..
                    }
                )
            })
        }));
    }

    #[test]
    fn async_try_await_throw_preserves_unwind_handler_edge() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun tick(): Int { return 1 }\nasync fun recover(): Int { try { val value: Int = await tick() throw value } catch (error: Int) { return 7 } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "recover")
            .expect("recover async MIR");
        let await_edge = body
            .blocks
            .iter()
            .find_map(|block| match block.terminator {
                mir::Terminator::Await {
                    unwind: Some(handler),
                    ..
                } => Some(handler),
                _ => None,
            })
            .expect("await unwind edge");
        assert!(matches!(
            body.blocks[await_edge].terminator,
            mir::Terminator::Goto { .. } | mir::Terminator::Return { .. }
        ));
        assert!(body.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                mir::Terminator::Throw {
                    target: Some(_),
                    ..
                }
            )
        }));
    }

    #[test]
    fn checked_async_mir_lowers_simple_catch_region_to_handler_edge() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun recover(): Int { try { throw \"boom\" } catch (error: String) { return 7 } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .async_mir
            .iter()
            .find(|body| body.name == "recover")
            .expect("recover MIR");
        assert!(body.blocks.iter().any(|block| {
            block
                .statements
                .iter()
                .any(|statement| matches!(statement, mir::Statement::EnterTry { .. }))
        }));
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::EnterTry {
                        catch_ty: Some(Ty::String),
                        ..
                    }
                )
            })
        }));
        assert!(body.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                mir::Terminator::Throw {
                    target: Some(_),
                    ..
                }
            )
        }));
        assert!(body.validate().is_ok());
    }

    #[test]
    fn checked_program_extracts_exception_regions_without_setjmp() {
        let file = aura_parser::parse_file(
            "package demo\nfun f(): Unit { try { throw \"boom\" } catch (error: String) { } finally { } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        assert_eq!(program.checked().exception_regions.len(), 1);
        let region = &program.checked().exception_regions[0];
        assert_eq!(region.owner, "f");
        assert!(region.catch_span.is_some());
        assert!(region.finally_span.is_some());
        assert!(region.has_throw);
    }

    #[test]
    fn generic_instantiations_are_normalized_before_backend_selection() {
        let file = aura_parser::parse_file(
            "package demo\nfun id<T>(x: T): T { return x }\nfun main() { id(\"value\") }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        assert!(program.checked().generic_instantiations.iter().any(|item| {
            item.owner == "id"
                && item.args == vec![Ty::String]
                && item.kind == GenericOwnerKind::Function
        }));
        let instance = program
            .checked()
            .generic_functions
            .iter()
            .find(|function| function.name.starts_with("id_"))
            .expect("closed generic function MIR");
        assert!(instance.body.is_some());
        assert!(program.checked().generic_function_mir_unlowered.is_empty());
        assert_eq!(instance.params[0].ty, Ty::String);
        assert_eq!(instance.ret.ty, Ty::String);
    }

    #[test]
    fn generic_instantiation_closure_substitutes_nested_type_parameters() {
        let file = aura_parser::parse_file(
            "package demo\nfun id<T>(x: T): T { return x }\nfun outer<U>(x: U): U { return id(x) }\nfun main() { outer(\"value\") }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        assert!(program.checked().generic_instantiations.iter().any(|item| {
            item.owner == "id"
                && item.kind == GenericOwnerKind::Function
                && item.args == vec![Ty::String]
        }));
    }

    #[test]
    fn checked_sync_mir_preserves_finally_edge_for_throwing_region() {
        let file = aura_parser::parse_file(
            "package demo\nfun recover(): Int { try { throw \"boom\" } catch (error: String) { return 7 } finally { } return 0 }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "recover")
            .and_then(|function| function.body.as_ref())
            .expect("recover MIR");
        assert!(body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement,
                    mir::Statement::EnterTry {
                        finally: Some(_),
                        ..
                    }
                )
            })
        }));
        assert!(body
            .blocks
            .iter()
            .any(|block| { matches!(block.terminator, mir::Terminator::Goto { .. }) }));
    }

    #[test]
    fn generic_async_instances_publish_concrete_mir() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun id<T>(value: T): T { return value }\nfun main() { id(7) }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .generic_async_mir
            .iter()
            .find(|body| body.name.starts_with("id_"))
            .expect("closed generic async MIR");
        assert_eq!(body.locals[0].ty, Ty::Int);
        assert_eq!(body.return_ty, Ty::Int);
        assert!(program.checked().generic_async_mir_unlowered.is_empty());
        assert!(state_machine::StateMachine::from_mir(body).is_ok());
        assert!(program
            .checked()
            .generic_async_state_machines
            .iter()
            .any(|machine| machine.function == body.name));
    }

    #[test]
    fn open_generic_async_identity_publishes_symbolic_mir_and_state_machine() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun identity<T>(value: T): T { return value }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .open_generic_async_mir
            .iter()
            .find(|body| body.name == "identity")
            .expect("open generic MIR");
        assert_eq!(body.locals[0].ty, Ty::TypeParam("T".into()));
        assert_eq!(body.return_ty, Ty::TypeParam("T".into()));
        assert!(program
            .checked()
            .open_generic_async_state_machines
            .iter()
            .any(|machine| machine.function == "identity"));
        assert!(program
            .checked()
            .open_generic_async_mir_unlowered
            .is_empty());
    }

    #[test]
    fn open_generic_async_await_publishes_symbolic_resume_edge() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun tick<T>(value: T): T { return value }\nasync fun forward<T>(value: T): T { val result: T = await tick(value) return result }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let body = program
            .checked()
            .open_generic_async_mir
            .iter()
            .find(|body| body.name == "forward")
            .expect("open generic await MIR");
        assert!(body
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, mir::Terminator::Await { .. })));
        assert!(program
            .checked()
            .open_generic_async_state_machines
            .iter()
            .any(|machine| machine.function == "forward"));
        assert!(program
            .checked()
            .open_generic_async_mir_unlowered
            .is_empty());
    }

    #[test]
    fn generic_unit_instances_are_not_dropped_from_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun touch<T>(value: T) { }\nfun main() { touch(7) }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("semantic check");
        let program = LoweredProgram::from_checked(checked);
        let instance = program
            .checked()
            .generic_functions
            .iter()
            .find(|function| function.name.starts_with("touch_"))
            .expect("unit generic function MIR");
        assert_eq!(instance.ret.ty, Ty::Unit);
        assert!(instance.body.is_some());
    }
}
