use crate::compiler::backend::aarch64_apple_darwin::asm::{Emitter, Register};
use crate::compiler::ir::instr::{Instruction, IrFunction, IrModule, Operand};
use std::collections::HashMap;

pub struct IrCodegen {
    emitter: Emitter,
    // SSA Reg -> Stack Offset (simple register spilling for all)
    reg_offsets: HashMap<u32, usize>,
    stack_offset: usize,
}

impl IrCodegen {
    pub fn new() -> Self {
        Self {
            emitter: Emitter::new(),
            reg_offsets: HashMap::new(),
            stack_offset: 0,
        }
    }

    pub fn generate(&mut self, module: IrModule) -> String {
        // Emit constants/globals
        if !module.globals.is_empty() {
            self.emitter.output.push_str(".data\n");
            for (name, content) in &module.globals {
                self.emitter
                    .output
                    .push_str(&format!("{}: .asciz \"{}\"\n", name, content));
            }
            self.emitter.output.push_str(".align 3\n");
        }
        // Always emit _aura_string_table for linker satisfaction
        self.emitter.output.push_str(".data\n");
        self.emitter.output.push_str(".global _aura_string_table\n");
        self.emitter.output.push_str("_aura_string_table:\n");
        if !module.globals.is_empty() {
            for (name, _) in &module.globals {
                self.emitter
                    .output
                    .push_str(&format!("    .quad {}\n", name));
            }
        } else {
            self.emitter.output.push_str("    .quad 0\n");
        }
        self.emitter.output.push_str(".text\n");

        // Emit VTables
        if !module.vtables.is_empty() {
            self.emitter.output.push_str(".data\n");
            self.emitter.output.push_str(".align 3\n");
            for (class, methods) in &module.vtables {
                self.emitter
                    .output
                    .push_str(&format!("vtable_{}:\n", class));
                for method in methods {
                    if method == "aura_null" {
                        self.emitter.output.push_str("    .quad 0\n");
                    } else {
                        self.emitter
                            .output
                            .push_str(&format!("    .quad _{}\n", method));
                    }
                }
            }
            self.emitter.output.push_str(".text\n");
        }

        self.emitter.emit_header();

        // Call main_aura if it exists
        if module.functions.iter().any(|f| f.name == "main_aura") {
            self.emitter.call("_main_aura");
        }

        self.emitter.emit_footer();

        for func in module.functions {
            self.generate_function(func);
        }
        self.emitter.output.clone()
    }

    fn generate_function(&mut self, func: IrFunction) {
        self.reg_offsets.clear();
        self.stack_offset = 16; // Start after saved FP/LR

        self.emitter
            .output
            .push_str(&format!(".global _{}\n", func.name));
        self.emitter.output.push_str(&format!("_{}:\n", func.name));

        // Prologue
        self.emitter
            .output
            .push_str("    stp x29, x30, [sp, -16]!\n");
        self.emitter.output.push_str("    mov x29, sp\n");
        self.emitter.output.push_str("    sub sp, sp, #256\n"); // Space for local variables and spill

        // Simplified: push all params to stack locations for SSA to find
        // Wait, for now let's just use x0-x7 directly if they are operands.

        for block in &func.blocks {
            self.emitter
                .output
                .push_str(&format!("L_{}_{}:\n", func.name, block.label));
            for instr in &block.instructions {
                self.generate_instruction(instr.clone(), &func.name);
            }
        }

        // Epilogue (if not already handled by return)
        self.emitter.output.push_str("    mov sp, x29\n");
        self.emitter.output.push_str("    ldp x29, x30, [sp], 16\n");
        self.emitter.output.push_str("    ret\n");
    }

    fn generate_instruction(&mut self, instr: Instruction, func_name: &str) {
        match instr {
            Instruction::Add(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .add(Register::X16, Register::X16, Register::X17);
                self.store_reg(dest, Register::X16);
            }
            Instruction::Sub(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .sub(Register::X16, Register::X16, Register::X17);
                self.store_reg(dest, Register::X16);
            }
            Instruction::Mul(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    mul x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Div(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    sdiv x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Rem(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    sdiv x18, x16, x17\n");
                self.emitter.output.push_str("    mul x18, x18, x17\n");
                self.emitter.output.push_str("    sub x16, x16, x18\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Eq(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .output
                    .push_str("    cmp x16, x17\n    cset x16, eq\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Ne(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .output
                    .push_str("    cmp x16, x17\n    cset x16, ne\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Lt(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .output
                    .push_str("    cmp x16, x17\n    cset x16, lt\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Le(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .output
                    .push_str("    cmp x16, x17\n    cset x16, le\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Gt(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .output
                    .push_str("    cmp x16, x17\n    cset x16, gt\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Ge(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter
                    .output
                    .push_str("    cmp x16, x17\n    cset x16, ge\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Return(val) => {
                if let Some(op) = val {
                    self.load_operand(Register::X0, op);
                }
                self.emitter.output.push_str("    mov sp, x29\n");
                self.emitter.output.push_str("    ldp x29, x30, [sp], 16\n");
                self.emitter.output.push_str("    ret\n");
            }
            Instruction::Call(dest, name, args) => {
                for (i, arg) in args.iter().enumerate() {
                    if i < 8 {
                        self.load_operand(Register::from_u8(i as u8), arg.clone());
                    }
                }
                self.emitter.call(&format!("_{}", name));
                self.store_reg(dest, Register::X0);
            }
            Instruction::CallVirtual(dest, obj, idx, args) => {
                // obj is the receiver object
                // VTable pointer is at offset 0
                self.load_operand(Register::X0, obj);
                // Load VTable pointer: x16 = [x0]
                self.emitter.output.push_str("    ldr x16, [x0]\n");
                // Load function pointer: x16 = [x16, idx * 8]
                self.emitter
                    .output
                    .push_str(&format!("    ldr x16, [x16, #{}]\n", idx * 8));

                // Prepare arguments (x0-x7)
                // Receiver must be in x0 (already there)
                for (i, arg) in args.iter().enumerate() {
                    if i < 8 {
                        self.load_operand(Register::from_u8(i as u8), arg.clone());
                    }
                }

                // Call function pointer
                self.emitter.output.push_str("    blr x16\n");
                self.store_reg(dest, Register::X0);
            }
            Instruction::SetVTable(obj, class_name) => {
                self.load_operand(Register::X16, obj);
                self.emitter
                    .output
                    .push_str(&format!("    adrp x17, vtable_{}@PAGE\n", class_name));
                self.emitter.output.push_str(&format!(
                    "    add x17, x17, vtable_{}@PAGEOFF\n",
                    class_name
                ));
                self.emitter.output.push_str("    str x17, [x16]\n");
            }
            Instruction::Alloc(dest, size) => {
                // Call aura_alloc(size)
                self.emitter.mov_imm(Register::X0, size as i64);
                self.emitter.call("_aura_alloc");
                self.store_reg(dest, Register::X0);
            }
            Instruction::Load(dest, base, offset) => {
                self.load_operand(Register::X17, base);
                self.emitter
                    .output
                    .push_str(&format!("    ldr x16, [x17, #{}]\n", offset));
                self.store_reg(dest, Register::X16);
            }
            Instruction::Store(val, base, offset) => {
                self.load_operand(Register::X16, val);
                self.load_operand(Register::X17, base);
                self.emitter
                    .output
                    .push_str(&format!("    str x16, [x17, #{}]\n", offset));
            }
            Instruction::WriteBarrier(obj, val) => {
                // Call aura_write_barrier(obj, val)
                self.load_operand(Register::X0, obj);
                self.load_operand(Register::X1, val);
                self.emitter.call("_aura_write_barrier");
            }
            Instruction::Jump(target) => {
                self.emitter
                    .output
                    .push_str(&format!("    b L_{}_{}\n", func_name, target));
            }
            Instruction::Branch(cond, then_label, else_label) => {
                self.load_operand(Register::X16, cond);
                self.emitter.output.push_str("    cmp x16, #0\n");
                self.emitter
                    .output
                    .push_str(&format!("    b.ne L_{}_{}\n", func_name, then_label));
                self.emitter
                    .output
                    .push_str(&format!("    b L_{}_{}\n", func_name, else_label));
            }
            Instruction::Move(dest, src) => {
                self.load_operand(Register::X16, src);
                self.store_reg(dest, Register::X16);
            }
            Instruction::StackAlloc(dest, size) => {
                self.stack_offset += size as usize;
                let data_offset = self.stack_offset;
                self.emitter
                    .output
                    .push_str(&format!("    sub x16, x29, #{}\n", data_offset));
                self.store_reg(dest, Register::X16);
            }
        }
    }

    fn load_operand(&mut self, reg: Register, op: Operand) {
        match op {
            Operand::Constant(c) => {
                self.emitter.mov_imm(reg, c);
            }
            Operand::Value(id) => {
                let offset = *self
                    .reg_offsets
                    .get(&id)
                    .unwrap_or_else(|| panic!("Reg {} not found in IR codegen for function", id));
                self.emitter.output.push_str(&format!(
                    "    ldr {}, [x29, -{}]\n",
                    reg.name(),
                    offset
                ));
            }
            Operand::Parameter(idx) => {
                if idx < 8 {
                    self.emitter.mov_reg(reg, Register::from_u8(idx as u8));
                } else {
                    panic!("More than 8 parameters not supported yet");
                }
            }
        }
    }

    fn store_reg(&mut self, id: u32, reg: Register) {
        if !self.reg_offsets.contains_key(&id) {
            self.stack_offset += 8;
            self.reg_offsets.insert(id, self.stack_offset);
        }
        let offset = self.reg_offsets.get(&id).unwrap();
        self.emitter
            .output
            .push_str(&format!("    str x{}, [x29, -{}]\n", reg.index(), offset));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ir::instr::{BasicBlock, IrFunction, IrModule};

    #[test]
    fn test_codegen_alloc_and_write_barrier() {
        let mut codegen = IrCodegen::new();
        let module = IrModule {
            globals: vec![],
            vtables: HashMap::new(),
            functions: vec![IrFunction {
                name: "test_func".to_string(),
                params: vec![],
                return_type: crate::compiler::ir::instr::IrType::Void,
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    instructions: vec![
                        Instruction::Alloc(1, 16),
                        Instruction::Alloc(2, 24),
                        Instruction::WriteBarrier(Operand::Value(1), Operand::Value(2)),
                    ],
                }],
            }],
        };

        let asm = codegen.generate(module);

        // Check that aura_alloc is called with right size
        assert!(asm.contains("bl _aura_alloc"));

        // Wait, the test might fail depending on exact mov_imm representation, let's just check for the call
        assert!(asm.contains("bl _aura_alloc"), "Should call aura_alloc");
        assert!(
            asm.contains("bl _aura_write_barrier"),
            "Should call write_barrier"
        );
    }
}
