//! Small, target-neutral MIR optimizations.
//!
//! The passes deliberately preserve ownership and control-flow statements. They
//! only rewrite literal expressions, unreachable blocks, and unused pure values.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::mir::{BasicBlock, BinaryOp, MirBody, Place, Rvalue, Statement, Terminator, UnaryOp};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Constant {
    Int(i64),
    Float(u64),
    Bool(bool),
    String(String),
    Null,
}

impl Constant {
    fn rvalue(&self) -> Rvalue {
        match self {
            Self::Int(value) => Rvalue::ConstInt(*value),
            Self::Float(value) => Rvalue::ConstFloat(*value),
            Self::Bool(value) => Rvalue::ConstBool(*value),
            Self::String(value) => Rvalue::ConstString(value.clone()),
            Self::Null => Rvalue::ConstNull,
        }
    }
}

fn constant(value: &Rvalue) -> Option<Constant> {
    match value {
        Rvalue::ConstInt(value) => Some(Constant::Int(*value)),
        Rvalue::ConstFloat(value) => Some(Constant::Float(*value)),
        Rvalue::ConstBool(value) => Some(Constant::Bool(*value)),
        Rvalue::ConstString(value) => Some(Constant::String(value.clone())),
        Rvalue::ConstNull => Some(Constant::Null),
        _ => None,
    }
}

fn fold_unary(op: UnaryOp, value: &Constant) -> Option<Constant> {
    match (op, value) {
        (UnaryOp::Neg, Constant::Int(value)) => Some(Constant::Int(value.checked_neg()?)),
        (UnaryOp::Neg, Constant::Float(value)) => {
            Some(Constant::Float((-f64::from_bits(*value)).to_bits()))
        }
        (UnaryOp::Not, Constant::Bool(value)) => Some(Constant::Bool(!value)),
        _ => None,
    }
}

fn fold_binary(op: BinaryOp, left: &Constant, right: &Constant) -> Option<Constant> {
    match (op, left, right) {
        (BinaryOp::Add, Constant::Int(a), Constant::Int(b)) => {
            Some(Constant::Int(a.checked_add(*b)?))
        }
        (BinaryOp::Sub, Constant::Int(a), Constant::Int(b)) => {
            Some(Constant::Int(a.checked_sub(*b)?))
        }
        (BinaryOp::Mul, Constant::Int(a), Constant::Int(b)) => {
            Some(Constant::Int(a.checked_mul(*b)?))
        }
        (BinaryOp::Div, Constant::Int(a), Constant::Int(b)) => {
            Some(Constant::Int(a.checked_div(*b)?))
        }
        (BinaryOp::Rem, Constant::Int(a), Constant::Int(b)) => {
            Some(Constant::Int(a.checked_rem(*b)?))
        }
        (BinaryOp::Eq, a, b) => Some(Constant::Bool(a == b)),
        (BinaryOp::Ne, a, b) => Some(Constant::Bool(a != b)),
        (BinaryOp::And, Constant::Bool(a), Constant::Bool(b)) => Some(Constant::Bool(*a && *b)),
        (BinaryOp::Or, Constant::Bool(a), Constant::Bool(b)) => Some(Constant::Bool(*a || *b)),
        (BinaryOp::Coalesce, Constant::Null, value) => Some(value.clone()),
        (BinaryOp::Coalesce, value, _) => Some(value.clone()),
        _ => None,
    }
}

fn fold_rvalue(value: &Rvalue, known: &HashMap<usize, Constant>) -> Option<Constant> {
    match value {
        Rvalue::Use(Place { local }) => known.get(local).cloned(),
        Rvalue::Unary { op, operand } => fold_unary(*op, known.get(&operand.local)?),
        Rvalue::Binary { op, left, right } => {
            fold_binary(*op, known.get(&left.local)?, known.get(&right.local)?)
        }
        Rvalue::Select {
            condition,
            then_value,
            else_value,
        } => match known.get(&condition.local)? {
            Constant::Bool(true) => known.get(&then_value.local).cloned(),
            Constant::Bool(false) => known.get(&else_value.local).cloned(),
            _ => None,
        },
        _ => constant(value),
    }
}

fn invalidate(known: &mut HashMap<usize, Constant>, place: Place) {
    known.remove(&place.local);
}

fn fold_block(block: &mut BasicBlock) {
    let mut known = HashMap::new();
    for statement in &mut block.statements {
        match statement {
            Statement::Assign { place, value } => {
                if matches!(
                    value,
                    Rvalue::Call { .. }
                        | Rvalue::CallIndirect { .. }
                        | Rvalue::AsyncOp(_)
                        | Rvalue::Intrinsic(_)
                ) {
                    // Calls may mutate aliased locals, including boxed
                    // captures, so discard block-local constant facts.
                    known.clear();
                }
                let folded = fold_rvalue(value, &known);
                if let Some(folded_value) = folded {
                    *value = folded_value.rvalue();
                    known.insert(place.local, folded_value);
                } else {
                    invalidate(&mut known, *place);
                }
            }
            Statement::Move { to, .. }
            | Statement::Clone { to, .. }
            | Statement::Retain { to, .. } => {
                invalidate(&mut known, *to);
            }
            Statement::StoreField { .. } => known.clear(),
            _ => {}
        }
    }
    let folded_branch = match &block.terminator {
        Terminator::SwitchInt {
            condition,
            then_target,
            else_target,
        } => known.get(&condition.local).and_then(|value| match value {
            Constant::Bool(true) => Some(*then_target),
            Constant::Bool(false) => Some(*else_target),
            _ => None,
        }),
        _ => None,
    };
    if let Some(target) = folded_branch {
        block.terminator = Terminator::Goto { target };
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
            .chain([*otherwise])
            .collect(),
        Terminator::Await { resume, unwind, .. } => {
            unwind.iter().copied().chain([*resume]).collect()
        }
        Terminator::Throw { target, .. } => target.iter().copied().collect(),
        Terminator::Return { .. } | Terminator::Cancel | Terminator::Unreachable => Vec::new(),
    }
}

fn simplify_cfg(body: &mut MirBody) {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([body.entry]);
    while let Some(block) = queue.pop_front() {
        if !reachable.insert(block) {
            continue;
        }
        for statement in &body.blocks[block].statements {
            if let Statement::EnterTry {
                handler, finally, ..
            } = statement
            {
                queue.push_back(*handler);
                if let Some(finally) = finally {
                    queue.push_back(*finally);
                }
            }
        }
        queue.extend(successors(&body.blocks[block].terminator));
    }
    if reachable.len() == body.blocks.len() {
        return;
    }

    let mut remap = HashMap::new();
    let mut blocks = Vec::with_capacity(reachable.len());
    for (old, block) in body.blocks.iter().enumerate() {
        if reachable.contains(&old) {
            let new = blocks.len();
            remap.insert(old, new);
            blocks.push(block.clone());
        }
    }
    for block in &mut blocks {
        for statement in &mut block.statements {
            if let Statement::EnterTry {
                handler, finally, ..
            } = statement
            {
                *handler = remap[handler];
                if let Some(finally) = finally {
                    *finally = remap[finally];
                }
            }
        }
        match &mut block.terminator {
            Terminator::Goto { target } => *target = remap[target],
            Terminator::SwitchInt {
                then_target,
                else_target,
                ..
            } => {
                *then_target = remap[then_target];
                *else_target = remap[else_target];
            }
            Terminator::SwitchTag {
                targets, otherwise, ..
            } => {
                for (_, target) in targets {
                    *target = remap[target];
                }
                *otherwise = remap[otherwise];
            }
            Terminator::Await { resume, unwind, .. } => {
                *resume = remap[resume];
                if let Some(target) = unwind {
                    *target = remap[target];
                }
            }
            Terminator::Throw {
                target: Some(target),
                ..
            } => *target = remap[target],
            _ => {}
        }
    }
    body.entry = remap[&body.entry];
    body.blocks = blocks;
}

fn referenced_places(body: &MirBody) -> HashSet<usize> {
    let mut used = HashSet::new();
    for block in &body.blocks {
        for statement in &block.statements {
            match statement {
                Statement::Assign { value, .. } | Statement::Evaluate(value) => {
                    references_in_rvalue(value, &mut used)
                }
                Statement::Move { from, .. }
                | Statement::Clone { from, .. }
                | Statement::Retain { from, .. }
                | Statement::Drop(from) => {
                    used.insert(from.local);
                }
                Statement::ExtractVariantField { operand, .. } => {
                    used.insert(operand.local);
                }
                Statement::LoadIndex {
                    collection, index, ..
                } => {
                    used.extend([collection.local, index.local]);
                }
                Statement::StoreField { object, value, .. } => {
                    used.extend([object.local, value.local]);
                }
                Statement::EnterTry { .. } | Statement::LeaveTry => {}
            }
        }
        match &block.terminator {
            Terminator::SwitchInt { condition, .. }
            | Terminator::SwitchTag {
                discriminant: condition,
                ..
            } => {
                used.insert(condition.local);
            }
            Terminator::Await { task, result, .. } => {
                used.extend([task.local, result.local]);
            }
            Terminator::Return { value: Some(value) } => {
                used.insert(value.local);
            }
            Terminator::Throw { value, .. } => {
                used.insert(value.local);
            }
            _ => {}
        }
    }
    used
}

fn references_in_rvalue(value: &Rvalue, used: &mut HashSet<usize>) {
    match value {
        Rvalue::Use(place)
        | Rvalue::Unary { operand: place, .. }
        | Rvalue::Length(place)
        | Rvalue::VariantTag { operand: place }
        | Rvalue::Unwrap { operand: place }
        | Rvalue::TypeTest { operand: place, .. } => {
            used.insert(place.local);
        }
        Rvalue::Binary { left, right, .. } => {
            used.extend([left.local, right.local]);
        }
        Rvalue::Select {
            condition,
            then_value,
            else_value,
        } => {
            used.extend([condition.local, then_value.local, else_value.local]);
        }
        Rvalue::Index { collection, index } => {
            used.extend([collection.local, index.local]);
        }
        Rvalue::Field { object, .. } => {
            used.insert(object.local);
        }
        Rvalue::Call { args, .. } => {
            used.extend(args.iter().map(|place| place.local));
        }
        Rvalue::CallIndirect { callee, args } => {
            used.insert(callee.local);
            used.extend(args.iter().map(|place| place.local));
        }
        Rvalue::AsyncOp(crate::mir::AsyncOp::Spawn { captures, .. }) => {
            used.extend(captures.iter().map(|capture| capture.source.local));
        }
        Rvalue::Function { captures, .. } => {
            used.extend(captures.iter().map(|capture| capture.source.local));
        }
        Rvalue::AsyncOp(_)
        | Rvalue::Intrinsic(_)
        | Rvalue::ConstInt(_)
        | Rvalue::ConstFloat(_)
        | Rvalue::ConstBool(_)
        | Rvalue::ConstString(_)
        | Rvalue::ConstNull => {}
    }
}

fn is_pure(value: &Rvalue) -> bool {
    matches!(
        value,
        Rvalue::Use(_)
            | Rvalue::ConstInt(_)
            | Rvalue::ConstFloat(_)
            | Rvalue::ConstBool(_)
            | Rvalue::ConstString(_)
            | Rvalue::ConstNull
            | Rvalue::Unary { .. }
            | Rvalue::Binary { .. }
            | Rvalue::Select { .. }
            | Rvalue::Unwrap { .. }
            | Rvalue::TypeTest { .. }
            | Rvalue::VariantTag { .. }
            | Rvalue::Length(_)
            | Rvalue::Index { .. }
            | Rvalue::Field { .. }
    )
}

fn eliminate_dead_assignments(body: &mut MirBody) {
    let used = referenced_places(body);
    for block in &mut body.blocks {
        block.statements.retain(|statement| match statement {
            Statement::Assign { place, value } => used.contains(&place.local) || !is_pure(value),
            _ => true,
        });
    }
}

/// Optimize one MIR body and recursively optimize nested spawned bodies.
pub fn optimize(body: &mut MirBody) {
    for block in &mut body.blocks {
        for statement in &mut block.statements {
            let value = match statement {
                Statement::Assign { value, .. } | Statement::Evaluate(value) => value,
                _ => continue,
            };
            if let Rvalue::AsyncOp(crate::mir::AsyncOp::Spawn { body, .. }) = value {
                optimize(body);
            }
        }
        fold_block(block);
    }
    simplify_cfg(body);
    eliminate_dead_assignments(body);
}

#[cfg(test)]
mod tests {
    use super::optimize;
    use crate::{mir::*, Effect, OwnershipMode, Ty};

    fn body(blocks: Vec<BasicBlock>) -> MirBody {
        MirBody {
            name: "test".into(),
            locals: vec![Local {
                name: "x".into(),
                ty: Ty::Int,
                ownership: OwnershipMode::Borrowed,
            }],
            blocks,
            entry: 0,
            return_ty: Ty::Int,
            effect: Effect::Pure,
        }
    }

    #[test]
    fn folds_constants_and_removes_dead_assignment() {
        let mut mir = body(vec![BasicBlock {
            statements: vec![
                Statement::Assign {
                    place: Place { local: 0 },
                    value: Rvalue::ConstInt(1),
                },
                Statement::Assign {
                    place: Place { local: 0 },
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        left: Place { local: 0 },
                        right: Place { local: 0 },
                    },
                },
            ],
            terminator: Terminator::Return { value: None },
        }]);
        optimize(&mut mir);
        assert!(mir.blocks[0].statements.is_empty());
    }

    #[test]
    fn removes_unreachable_cfg_blocks() {
        let mut mir = body(vec![
            BasicBlock {
                statements: vec![],
                terminator: Terminator::Return { value: None },
            },
            BasicBlock {
                statements: vec![],
                terminator: Terminator::Unreachable,
            },
        ]);
        optimize(&mut mir);
        assert_eq!(mir.blocks.len(), 1);
        mir.validate().unwrap();
    }

    #[test]
    fn preserves_values_captured_by_function_and_spawn_rvalues() {
        let capture = ClosureCapture {
            source: Place { local: 0 },
            ty: Ty::Int,
            by_ref: false,
        };
        let mut mir = MirBody {
            name: "capture".into(),
            locals: vec![
                Local {
                    name: "value".into(),
                    ty: Ty::Int,
                    ownership: OwnershipMode::Borrowed,
                },
                Local {
                    name: "closure".into(),
                    ty: Ty::Fun {
                        params: vec![],
                        ret: Box::new(Ty::Int),
                    },
                    ownership: OwnershipMode::Borrowed,
                },
            ],
            blocks: vec![BasicBlock {
                statements: vec![
                    Statement::Assign {
                        place: Place { local: 0 },
                        value: Rvalue::ConstInt(42),
                    },
                    Statement::Assign {
                        place: Place { local: 1 },
                        value: Rvalue::Function {
                            name: "__lambda_1".into(),
                            captures: vec![capture],
                        },
                    },
                ],
                terminator: Terminator::Return {
                    value: Some(Place { local: 0 }),
                },
            }],
            entry: 0,
            return_ty: Ty::Int,
            effect: Effect::Pure,
        };
        optimize(&mut mir);
        assert!(matches!(
            mir.blocks[0].statements.first(),
            Some(Statement::Assign {
                place: Place { local: 0 },
                value: Rvalue::ConstInt(42)
            })
        ));
    }
}
