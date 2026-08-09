//! Target-neutral mid-level IR data model.

pub use aura_ownership::OwnershipMode;

pub mod opt;
use aura_sema::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Pure,
    Async,
    Throws,
}
pub mod state_machine {
    use std::collections::BTreeSet;

    use super::mir::{AsyncOp, MirBody, Place, Rvalue, Statement, Terminator};

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
        /// Locals that remain live after this suspension. Backends use this
        /// typed set to build the frame storage map instead of scanning all
        /// locals conservatively.
        pub live_locals: Vec<usize>,
        pub ownership: Vec<OwnershipTransfer>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OwnershipTransfer {
        pub local: usize,
        pub action: aura_ownership::Action,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum BuildError {
        InvalidMir,
    }

    fn add_place(places: &mut BTreeSet<usize>, place: Place) {
        places.insert(place.local);
    }

    fn read_place(place: Place, uses: &mut BTreeSet<usize>, defs: &BTreeSet<usize>) {
        if !defs.contains(&place.local) {
            add_place(uses, place);
        }
    }

    fn rvalue_uses(value: &Rvalue, uses: &mut BTreeSet<usize>) {
        match value {
            Rvalue::Use(place)
            | Rvalue::Unwrap { operand: place }
            | Rvalue::TypeTest { operand: place, .. }
            | Rvalue::VariantTag { operand: place }
            | Rvalue::Length(place)
            | Rvalue::Field { object: place, .. }
            | Rvalue::AsyncOp(AsyncOp::Join(place))
            | Rvalue::AsyncOp(AsyncOp::Cancel(place))
            | Rvalue::AsyncOp(AsyncOp::ChannelReceive(place))
            | Rvalue::AsyncOp(AsyncOp::ChannelClose(place)) => add_place(uses, *place),
            Rvalue::Unary { operand, .. } => add_place(uses, *operand),
            Rvalue::Binary { left, right, .. } => {
                add_place(uses, *left);
                add_place(uses, *right);
            }
            Rvalue::Select {
                condition,
                then_value,
                else_value,
            } => {
                add_place(uses, *condition);
                add_place(uses, *then_value);
                add_place(uses, *else_value);
            }
            Rvalue::Index { collection, index } => {
                add_place(uses, *collection);
                add_place(uses, *index);
            }
            Rvalue::Intrinsic(_)
            | Rvalue::ConstInt(_)
            | Rvalue::ConstFloat(_)
            | Rvalue::ConstBool(_)
            | Rvalue::ConstString(_)
            | Rvalue::ConstNull => {}
            Rvalue::Function { captures, .. } => {
                for capture in captures {
                    add_place(uses, capture.source);
                }
            }
            Rvalue::AsyncOp(AsyncOp::Spawn { captures, .. }) => {
                for capture in captures {
                    add_place(uses, capture.source);
                }
            }
            Rvalue::AsyncOp(AsyncOp::ChannelCreate { capacity, .. }) => {
                add_place(uses, *capacity);
            }
            Rvalue::AsyncOp(AsyncOp::ChannelSend { channel, value }) => {
                add_place(uses, *channel);
                add_place(uses, *value);
            }
            Rvalue::Call { args, .. } => {
                for arg in args {
                    add_place(uses, *arg);
                }
            }
            Rvalue::CallIndirect { callee, args } => {
                add_place(uses, *callee);
                for arg in args {
                    add_place(uses, *arg);
                }
            }
        }
    }

    fn statement_uses_defs(
        statement: &Statement,
        uses: &mut BTreeSet<usize>,
        defs: &mut BTreeSet<usize>,
    ) {
        match statement {
            Statement::Assign { place, value } => {
                let mut value_uses = BTreeSet::new();
                rvalue_uses(value, &mut value_uses);
                for local in value_uses {
                    read_place(Place { local }, uses, defs);
                }
                defs.insert(place.local);
            }
            Statement::Evaluate(value) => {
                let mut value_uses = BTreeSet::new();
                rvalue_uses(value, &mut value_uses);
                for local in value_uses {
                    read_place(Place { local }, uses, defs);
                }
            }
            Statement::Move { from, to }
            | Statement::Clone { from, to }
            | Statement::Retain { from, to } => {
                read_place(*from, uses, defs);
                defs.insert(to.local);
            }
            Statement::ExtractVariantField { operand, to, .. }
            | Statement::LoadIndex {
                collection: operand,
                to,
                ..
            } => {
                read_place(*operand, uses, defs);
                defs.insert(to.local);
            }
            Statement::StoreField { object, value, .. } => {
                read_place(*object, uses, defs);
                read_place(*value, uses, defs);
            }
            Statement::Drop(place) => read_place(*place, uses, defs),
            Statement::EnterTry { .. } | Statement::LeaveTry => {}
        }
    }

    fn terminator_uses_defs(
        terminator: &Terminator,
        uses: &mut BTreeSet<usize>,
        defs: &mut BTreeSet<usize>,
    ) {
        match terminator {
            Terminator::SwitchInt { condition, .. }
            | Terminator::SwitchTag {
                discriminant: condition,
                ..
            }
            | Terminator::Throw {
                value: condition, ..
            }
            | Terminator::Return {
                value: Some(condition),
            } => read_place(*condition, uses, defs),
            Terminator::Await { task, result, .. } => {
                read_place(*task, uses, defs);
                defs.insert(result.local);
            }
            Terminator::Goto { .. }
            | Terminator::Return { value: None }
            | Terminator::Cancel
            | Terminator::Unreachable => {}
        }
    }

    fn successors(terminator: &Terminator) -> Vec<usize> {
        match terminator {
            Terminator::Goto { target } => vec![*target],
            Terminator::SwitchInt {
                then_target,
                else_target,
                ..
            } => vec![*then_target, *else_target],
            Terminator::SwitchTag {
                targets, otherwise, ..
            } => targets
                .iter()
                .map(|(_, target)| *target)
                .chain(std::iter::once(*otherwise))
                .collect(),
            Terminator::Await { resume, unwind, .. } => std::iter::once(*resume)
                .chain(unwind.iter().copied())
                .collect(),
            Terminator::Throw { target, .. } => target.iter().copied().collect(),
            Terminator::Return { .. } | Terminator::Cancel | Terminator::Unreachable => Vec::new(),
        }
    }

    fn liveness(body: &MirBody) -> (Vec<BTreeSet<usize>>, Vec<BTreeSet<usize>>) {
        let mut block_uses = Vec::with_capacity(body.blocks.len());
        let mut block_defs = Vec::with_capacity(body.blocks.len());
        for block in &body.blocks {
            let mut uses = BTreeSet::new();
            let mut defs = BTreeSet::new();
            for statement in &block.statements {
                statement_uses_defs(statement, &mut uses, &mut defs);
            }
            terminator_uses_defs(&block.terminator, &mut uses, &mut defs);
            block_uses.push(uses);
            block_defs.push(defs);
        }
        let mut live_in = vec![BTreeSet::new(); body.blocks.len()];
        let mut live_out = vec![BTreeSet::new(); body.blocks.len()];
        loop {
            let mut changed = false;
            for block_index in (0..body.blocks.len()).rev() {
                let mut out = BTreeSet::new();
                for successor in successors(&body.blocks[block_index].terminator) {
                    out.extend(live_in[successor].iter().copied());
                }
                let mut input = block_uses[block_index].clone();
                input.extend(
                    out.iter()
                        .filter(|local| !block_defs[block_index].contains(local))
                        .copied(),
                );
                changed |= live_out[block_index] != out || live_in[block_index] != input;
                live_out[block_index] = out;
                live_in[block_index] = input;
            }
            if !changed {
                return (live_in, live_out);
            }
        }
    }

    impl StateMachine {
        pub fn from_mir(body: &MirBody) -> Result<Self, BuildError> {
            body.validate().map_err(|_| BuildError::InvalidMir)?;
            let (_, live_out) = liveness(body);
            let mut frame_locals = BTreeSet::new();
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
                            let mut live_locals = live_out[block].clone();
                            live_locals.insert(task.local);
                            // The await result is written before the resumed
                            // state can run its first drop/mark boundary.
                            live_locals.insert(result.local);
                            frame_locals.extend(live_locals.iter().copied());
                            let live_local_set = live_locals.clone();
                            Some(Suspension {
                                task_local: task.local,
                                result_local: result.local,
                                resume: *resume,
                                unwind: *unwind,
                                live_locals: live_locals.into_iter().collect(),
                                ownership: body
                                    .locals
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(local, value)| {
                                        if !live_local_set.contains(&local) {
                                            return None;
                                        }
                                        let action =
                                            aura_ownership::plan_for_ty(&value.ty).across_suspend;
                                        (!matches!(
                                            action,
                                            aura_ownership::Action::Copy
                                                | aura_ownership::Action::Noop
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
            Ok(Self {
                function: body.name.clone(),
                entry: body.entry,
                frame_locals: frame_locals.into_iter().collect(),
                states,
            })
        }
    }
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

#[cfg(test)]
mod tests {
    use super::mir::{BasicBlock, Local, MirBody, Place, Rvalue, Statement, Terminator};
    use super::state_machine::StateMachine;
    use super::Effect;
    use aura_ownership::OwnershipMode;
    use aura_sema::Ty;

    #[test]
    fn suspension_frame_map_contains_live_locals_and_transfer_slots() {
        let body = MirBody {
            name: "live_map".into(),
            locals: vec![
                Local {
                    name: "task".into(),
                    ty: Ty::Task(Box::new(Ty::Int)),
                    ownership: OwnershipMode::Owned,
                },
                Local {
                    name: "live".into(),
                    ty: Ty::Int,
                    ownership: OwnershipMode::Borrowed,
                },
                Local {
                    name: "dead".into(),
                    ty: Ty::Int,
                    ownership: OwnershipMode::Borrowed,
                },
                Local {
                    name: "result".into(),
                    ty: Ty::Int,
                    ownership: OwnershipMode::Borrowed,
                },
            ],
            blocks: vec![
                BasicBlock {
                    statements: vec![
                        Statement::Assign {
                            place: Place { local: 1 },
                            value: Rvalue::ConstInt(1),
                        },
                        Statement::Assign {
                            place: Place { local: 2 },
                            value: Rvalue::ConstInt(2),
                        },
                    ],
                    terminator: Terminator::Await {
                        task: Place { local: 0 },
                        result: Place { local: 3 },
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

        let machine = StateMachine::from_mir(&body).expect("valid MIR");
        assert_eq!(machine.frame_locals, vec![0, 1, 3]);
        assert_eq!(
            machine.states[0]
                .suspension
                .as_ref()
                .expect("await state")
                .live_locals,
            vec![0, 1, 3]
        );
    }
}
