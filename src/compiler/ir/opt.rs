use crate::compiler::ir::instr::{Instruction, IrModule, Operand};
use std::collections::HashMap;

pub struct Optimizer {}

impl Optimizer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn optimize(&mut self, mut module: IrModule) -> IrModule {
        for func in &mut module.functions {
            // Very simple constant folding + propagation per function
            let mut constants: HashMap<u32, i64> = HashMap::new();

            for block in &mut func.blocks {
                let mut new_instrs = Vec::new();
                for instr in block.instructions.drain(..) {
                    match instr {
                        Instruction::Add(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, l + r);
                            } else {
                                new_instrs.push(Instruction::Add(dest, left, right));
                            }
                        }
                        Instruction::Sub(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, l - r);
                            } else {
                                new_instrs.push(Instruction::Sub(dest, left, right));
                            }
                        }
                        Instruction::Mul(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, l * r);
                            } else {
                                new_instrs.push(Instruction::Mul(dest, left, right));
                            }
                        }
                        Instruction::Div(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                if *r != 0 {
                                    constants.insert(dest, l / r);
                                } else {
                                    new_instrs.push(Instruction::Div(dest, left, right));
                                }
                            } else {
                                new_instrs.push(Instruction::Div(dest, left, right));
                            }
                        }
                        Instruction::Rem(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                if *r != 0 {
                                    constants.insert(dest, l % r);
                                } else {
                                    new_instrs.push(Instruction::Rem(dest, left, right));
                                }
                            } else {
                                new_instrs.push(Instruction::Rem(dest, left, right));
                            }
                        }
                        Instruction::Eq(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, if l == r { 1 } else { 0 });
                            } else {
                                new_instrs.push(Instruction::Eq(dest, left, right));
                            }
                        }
                        Instruction::Ne(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, if l != r { 1 } else { 0 });
                            } else {
                                new_instrs.push(Instruction::Ne(dest, left, right));
                            }
                        }
                        Instruction::Lt(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, if l < r { 1 } else { 0 });
                            } else {
                                new_instrs.push(Instruction::Lt(dest, left, right));
                            }
                        }
                        Instruction::Le(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, if l <= r { 1 } else { 0 });
                            } else {
                                new_instrs.push(Instruction::Le(dest, left, right));
                            }
                        }
                        Instruction::Gt(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, if l > r { 1 } else { 0 });
                            } else {
                                new_instrs.push(Instruction::Gt(dest, left, right));
                            }
                        }
                        Instruction::Ge(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, if l >= r { 1 } else { 0 });
                            } else {
                                new_instrs.push(Instruction::Ge(dest, left, right));
                            }
                        }
                        Instruction::BitAnd(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, l & r);
                            } else {
                                new_instrs.push(Instruction::BitAnd(dest, left, right));
                            }
                        }
                        Instruction::BitOr(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, l | r);
                            } else {
                                new_instrs.push(Instruction::BitOr(dest, left, right));
                            }
                        }
                        Instruction::BitXor(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                constants.insert(dest, l ^ r);
                            } else {
                                new_instrs.push(Instruction::BitXor(dest, left, right));
                            }
                        }
                        Instruction::Shl(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                if *r >= 0 && *r < 64 {
                                    constants.insert(dest, l << r);
                                } else {
                                    new_instrs.push(Instruction::Shl(dest, left, right));
                                }
                            } else {
                                new_instrs.push(Instruction::Shl(dest, left, right));
                            }
                        }
                        Instruction::Shr(dest, lhs, rhs) => {
                            let left = self.resolve_operand(&lhs, &constants);
                            let right = self.resolve_operand(&rhs, &constants);
                            if let (Operand::Constant(l), Operand::Constant(r)) = (&left, &right) {
                                if *r >= 0 && *r < 64 {
                                    constants.insert(dest, l >> r);
                                } else {
                                    new_instrs.push(Instruction::Shr(dest, left, right));
                                }
                            } else {
                                new_instrs.push(Instruction::Shr(dest, left, right));
                            }
                        }
                        Instruction::BitNot(dest, src) => {
                            let val = self.resolve_operand(&src, &constants);
                            if let Operand::Constant(v) = val {
                                constants.insert(dest, !v);
                            } else {
                                new_instrs.push(Instruction::BitNot(dest, val));
                            }
                        }
                        Instruction::Alloc(dest, s) => {
                            new_instrs.push(Instruction::Alloc(dest, s));
                        }
                        Instruction::Jump(l) => {
                            new_instrs.push(Instruction::Jump(l));
                        }
                        Instruction::Call(dest, name, args) => {
                            let new_args = args
                                .into_iter()
                                .map(|a| self.resolve_operand(&a, &constants))
                                .collect();
                            new_instrs.push(Instruction::Call(dest, name, new_args));
                        }
                        Instruction::FCall(dest, name, args) => {
                            let new_args = args
                                .into_iter()
                                .map(|a| self.resolve_operand(&a, &constants))
                                .collect();
                            new_instrs.push(Instruction::FCall(dest, name, new_args));
                        }
                        Instruction::Return(val) => {
                            new_instrs.push(Instruction::Return(
                                val.map(|v| self.resolve_operand(&v, &constants)),
                            ));
                        }
                        Instruction::Branch(cond, t, e) => {
                            new_instrs.push(Instruction::Branch(
                                self.resolve_operand(&cond, &constants),
                                t,
                                e,
                            ));
                        }
                        Instruction::Store(val, base, off) => {
                            new_instrs.push(Instruction::Store(
                                self.resolve_operand(&val, &constants),
                                self.resolve_operand(&base, &constants),
                                off,
                            ));
                        }
                        Instruction::WriteBarrier(obj, val) => {
                            new_instrs.push(Instruction::WriteBarrier(
                                self.resolve_operand(&obj, &constants),
                                self.resolve_operand(&val, &constants),
                            ));
                        }
                        Instruction::Load(dest, base, off) => {
                            new_instrs.push(Instruction::Load(
                                dest,
                                self.resolve_operand(&base, &constants),
                                off,
                            ));
                        }
                        Instruction::CallVirtual(dest, obj, idx, args) => {
                            let new_obj = self.resolve_operand(&obj, &constants);
                            let new_args = args
                                .into_iter()
                                .map(|a| self.resolve_operand(&a, &constants))
                                .collect();
                            new_instrs.push(Instruction::CallVirtual(dest, new_obj, idx, new_args));
                        }
                        Instruction::SetVTable(obj, class) => {
                            new_instrs.push(Instruction::SetVTable(
                                self.resolve_operand(&obj, &constants),
                                class,
                            ));
                        }
                        Instruction::Move(dest, src) => {
                            new_instrs.push(Instruction::Move(
                                dest,
                                self.resolve_operand(&src, &constants),
                            ));
                        }
                        Instruction::StackAlloc(dest, size) => {
                            new_instrs.push(Instruction::StackAlloc(dest, size));
                        }
                        Instruction::FAdd(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FAdd(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FSub(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FSub(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FMul(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FMul(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FDiv(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FDiv(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FRem(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FRem(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FEq(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FEq(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FNe(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FNe(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FLt(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FLt(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FLe(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FLe(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FGt(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FGt(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::FGe(dest, lhs, rhs) => {
                            new_instrs.push(Instruction::FGe(
                                dest,
                                self.resolve_operand(&lhs, &constants),
                                self.resolve_operand(&rhs, &constants),
                            ));
                        }
                        Instruction::IToF(dest, src) => {
                            new_instrs.push(Instruction::IToF(
                                dest,
                                self.resolve_operand(&src, &constants),
                            ));
                        }
                        Instruction::FToI(dest, src) => {
                            new_instrs.push(Instruction::FToI(
                                dest,
                                self.resolve_operand(&src, &constants),
                            ));
                        }
                        Instruction::LoadVTableAddress(dest, class) => {
                            new_instrs.push(Instruction::LoadVTableAddress(dest, class));
                        }
                    }
                }
                block.instructions = new_instrs;
            }
        }
        module
    }

    fn resolve_operand(&self, op: &Operand, constants: &HashMap<u32, i64>) -> Operand {
        match op {
            Operand::Value(id) => {
                if let Some(&c) = constants.get(id) {
                    Operand::Constant(c)
                } else {
                    op.clone()
                }
            }
            Operand::FloatingConstant(_) => op.clone(),
            _ => op.clone(),
        }
    }
}
