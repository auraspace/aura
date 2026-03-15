use crate::compiler::ir::instr::{BasicBlock, Instruction, IrFunction, IrType, Operand};

pub struct IrBuilder {
    pub current_block: String,
    pub blocks: Vec<BasicBlock>,
    pub reg_count: u32,
    pub label_count: u32,
}

impl IrBuilder {
    pub fn new() -> Self {
        Self {
            current_block: "entry".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                instructions: Vec::new(),
            }],
            reg_count: 0,
            label_count: 0,
        }
    }

    pub fn new_reg(&mut self) -> u32 {
        let r = self.reg_count;
        self.reg_count += 1;
        r
    }

    pub fn new_label(&mut self, prefix: &str) -> String {
        let l = self.label_count;
        self.label_count += 1;
        format!("L_{}_{}", prefix, l)
    }

    pub fn create_block(&mut self, label: String) {
        self.blocks.push(BasicBlock {
            label,
            instructions: Vec::new(),
        });
    }

    pub fn set_block(&mut self, label: String) {
        if !self.blocks.iter().any(|b| b.label == label) {
            self.create_block(label.clone());
        }
        self.current_block = label;
    }

    pub fn emit(&mut self, instr: Instruction) {
        for b in &mut self.blocks {
            if b.label == self.current_block {
                b.instructions.push(instr);
                return;
            }
        }
    }

    pub fn add(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Add(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn sub(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Sub(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn mul(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Mul(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn div(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Div(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn rem(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Rem(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fadd(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FAdd(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fsub(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FSub(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fmul(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FMul(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fdiv(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FDiv(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn frem(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FRem(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn bit_and(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::BitAnd(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn bit_or(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::BitOr(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn bit_xor(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::BitXor(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn shl(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Shl(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn shr(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Shr(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn bit_not(&mut self, src: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::BitNot(dest, src));
        Operand::Value(dest)
    }

    pub fn eq(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Eq(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn ne(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Ne(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn lt(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Lt(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn le(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Le(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn gt(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Gt(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn ge(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Ge(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn feq(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FEq(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fne(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FNe(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn flt(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FLt(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fle(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FLe(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fgt(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FGt(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn fge(&mut self, lhs: Operand, rhs: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FGe(dest, lhs, rhs));
        Operand::Value(dest)
    }

    pub fn jump(&mut self, target: String) {
        self.emit(Instruction::Jump(target));
    }

    pub fn branch(&mut self, cond: Operand, then_block: String, else_block: String) {
        self.emit(Instruction::Branch(cond, then_block, else_block));
    }

    pub fn ret(&mut self, val: Option<Operand>) {
        self.emit(Instruction::Return(val));
    }

    pub fn call(&mut self, func: String, args: Vec<Operand>) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Call(dest, func, args));
        Operand::Value(dest)
    }

    pub fn call_virtual(&mut self, obj: Operand, idx: u32, args: Vec<Operand>) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::CallVirtual(dest, obj, idx, args));
        Operand::Value(dest)
    }

    pub fn mov(&mut self, src: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Move(dest, src));
        Operand::Value(dest)
    }

    pub fn itof(&mut self, src: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::IToF(dest, src));
        Operand::Value(dest)
    }

    pub fn ftoi(&mut self, src: Operand) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FToI(dest, src));
        Operand::Value(dest)
    }

    pub fn fcall(&mut self, name: String, args: Vec<Operand>) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::FCall(dest, name, args));
        Operand::Value(dest)
    }

    pub fn salloc(&mut self, size: u32) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::StackAlloc(dest, size));
        Operand::Value(dest)
    }

    pub fn store(&mut self, src: Operand, base: Operand, offset: u32) {
        self.emit(Instruction::Store(src, base, offset));
    }

    pub fn load(&mut self, base: Operand, offset: u32) -> Operand {
        let dest = self.new_reg();
        self.emit(Instruction::Load(dest, base, offset));
        Operand::Value(dest)
    }

    pub fn set_vtable(&mut self, obj: Operand, class_name: String) {
        self.emit(Instruction::SetVTable(obj, class_name));
    }

    pub fn finish_function(
        &mut self,
        name: String,
        params: Vec<IrType>,
        return_type: IrType,
    ) -> IrFunction {
        let f = IrFunction {
            name,
            params,
            return_type,
            blocks: self.blocks.split_off(0),
        };
        // Reset state for next function
        self.current_block = "entry".to_string();
        self.blocks = vec![BasicBlock {
            label: "entry".to_string(),
            instructions: Vec::new(),
        }];
        self.reg_count = 0;
        self.label_count = 0;
        f
    }
}
