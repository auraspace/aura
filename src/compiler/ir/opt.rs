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
                        Instruction::Load(dest, base, off) => {
                            new_instrs.push(Instruction::Load(
                                dest,
                                self.resolve_operand(&base, &constants),
                                off,
                            ));
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
            _ => op.clone(),
        }
    }
}
