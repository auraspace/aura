use crate::compiler::backend::x86_64::asm::{Emitter, Register};
use crate::compiler::ir::instr::{Instruction, IrFunction, IrModule, Operand};
use std::collections::HashMap;

pub struct IrCodegen {
    emitter: Emitter,
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
        if !module.globals.is_empty() {
            self.emitter.output.push_str(".data\n");
            for (name, content) in &module.globals {
                self.emitter
                    .output
                    .push_str(&format!("{}: .asciz \"{}\"\n", name, content));
            }
            // Align and create the string table for runtime FFI lookup
            self.emitter.output.push_str(".align 3\n");
            self.emitter.output.push_str(".global _aura_string_table\n");
            self.emitter.output.push_str("_aura_string_table:\n");
            for (name, _) in &module.globals {
                self.emitter
                    .output
                    .push_str(&format!("    .quad {}\n", name));
            }
            self.emitter.output.push_str(".text\n");
        } else {
            self.emitter.output.push_str(".text\n");
        }

        self.emitter.emit_header();

        self.emitter.emit_footer();

        for func in module.functions {
            self.generate_function(func);
        }
        self.emitter.output.clone()
    }

    fn generate_function(&mut self, func: IrFunction) {
        self.reg_offsets.clear();
        self.stack_offset = 0;

        self.emitter
            .output
            .push_str(&format!(".global _{}\n", func.name));
        self.emitter.output.push_str(&format!("_{}:\n", func.name));

        // Prologue
        self.emitter.output.push_str("    push rbp\n");
        self.emitter.output.push_str("    mov rbp, rsp\n");
        self.emitter.output.push_str("    sub rsp, 256\n");

        for block in &func.blocks {
            self.emitter
                .output
                .push_str(&format!("L_{}_{}:\n", func.name, block.label));
            for instr in &block.instructions {
                self.generate_instruction(instr.clone(), &func.name);
            }
        }

        // Epilogue
        self.emitter.output.push_str("    mov rsp, rbp\n");
        self.emitter.output.push_str("    pop rbp\n");
        self.emitter.output.push_str("    ret\n");
    }

    fn generate_instruction(&mut self, instr: Instruction, func_name: &str) {
        match instr {
            Instruction::Add(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.mov_reg(Register::RAX, Register::R10);
                self.emitter.add(Register::RAX, Register::R11);
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Sub(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.mov_reg(Register::RAX, Register::R10);
                self.emitter.sub(Register::RAX, Register::R11);
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Mul(dest, lhs, rhs) => {
                self.load_operand(Register::RAX, lhs);
                self.load_operand(Register::R10, rhs);
                self.emitter.output.push_str("    imul rax, r10\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Div(dest, lhs, rhs) => {
                self.load_operand(Register::RAX, lhs);
                self.load_operand(Register::R10, rhs);
                self.emitter.output.push_str("    cqo\n");
                self.emitter.output.push_str("    idiv r10\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Rem(dest, lhs, rhs) => {
                self.load_operand(Register::RAX, lhs);
                self.load_operand(Register::R10, rhs);
                self.emitter.output.push_str("    cqo\n");
                self.emitter.output.push_str("    idiv r10\n"); // Remainder in RDX
                self.store_reg(dest, Register::RDX);
            }
            Instruction::Eq(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.output.push_str("    cmp r10, r11\n");
                self.emitter.output.push_str("    sete al\n");
                self.emitter.output.push_str("    movzx rax, al\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Ne(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.output.push_str("    cmp r10, r11\n");
                self.emitter.output.push_str("    setne al\n");
                self.emitter.output.push_str("    movzx rax, al\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Lt(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.output.push_str("    cmp r10, r11\n");
                self.emitter.output.push_str("    setl al\n");
                self.emitter.output.push_str("    movzx rax, al\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Le(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.output.push_str("    cmp r10, r11\n");
                self.emitter.output.push_str("    setle al\n");
                self.emitter.output.push_str("    movzx rax, al\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Gt(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.output.push_str("    cmp r10, r11\n");
                self.emitter.output.push_str("    setg al\n");
                self.emitter.output.push_str("    movzx rax, al\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Ge(dest, lhs, rhs) => {
                self.load_operand(Register::R10, lhs);
                self.load_operand(Register::R11, rhs);
                self.emitter.output.push_str("    cmp r10, r11\n");
                self.emitter.output.push_str("    setge al\n");
                self.emitter.output.push_str("    movzx rax, al\n");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Jump(target) => {
                self.emitter
                    .output
                    .push_str(&format!("    jmp L_{}_{}\n", func_name, target));
            }
            Instruction::Branch(cond, then_label, else_label) => {
                self.load_operand(Register::R10, cond);
                self.emitter.output.push_str("    cmp r10, 0\n");
                self.emitter
                    .output
                    .push_str(&format!("    jne L_{}_{}\n", func_name, then_label));
                self.emitter
                    .output
                    .push_str(&format!("    jmp L_{}_{}\n", func_name, else_label));
            }
            Instruction::Return(val) => {
                if let Some(op) = val {
                    self.load_operand(Register::RAX, op);
                }
                self.emitter.output.push_str("    mov rsp, rbp\n");
                self.emitter.output.push_str("    pop rbp\n");
                self.emitter.output.push_str("    ret\n");
            }
            Instruction::Call(dest, name, args) => {
                // Ensure proper 16-byte alignment might be needed here, simplified for now
                for (i, arg) in args.iter().enumerate() {
                    let reg = Register::arg_index(i);
                    self.load_operand(reg, arg.clone());
                }
                // Avoid using _ for user methods, except stubs which come with _ internally?
                // Wait, aura maps to functions with _ prefixes in arm64, so let's match that.
                self.emitter.call(&format!("_{}", name));
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Alloc(dest, size) => {
                self.emitter.mov_imm(Register::RDI, size as i64);
                self.emitter.call("_aura_alloc");
                self.store_reg(dest, Register::RAX);
            }
            Instruction::Load(dest, base, offset) => {
                self.load_operand(Register::R10, base);
                self.emitter
                    .output
                    .push_str(&format!("    mov r11, [r10 + {}]\n", offset));
                self.store_reg(dest, Register::R11);
            }
            Instruction::Store(val, base, offset) => {
                self.load_operand(Register::R10, val);
                self.load_operand(Register::R11, base);
                self.emitter
                    .output
                    .push_str(&format!("    mov [r11 + {}], r10\n", offset));
            }
            Instruction::WriteBarrier(obj, val) => {
                self.load_operand(Register::RDI, obj);
                self.load_operand(Register::RSI, val);
                self.emitter.call("_aura_write_barrier");
            }
        }
    }

    fn load_operand(&mut self, reg: Register, op: Operand) {
        match op {
            Operand::Constant(c) => {
                self.emitter.mov_imm(reg, c);
            }
            Operand::Value(id) => {
                let offset = self
                    .reg_offsets
                    .get(&id)
                    .unwrap_or_else(|| panic!("Reg {} not found", id));
                self.emitter.output.push_str(&format!(
                    "    mov {}, [rbp - {}]\n",
                    reg.name(),
                    offset
                ));
            }
            Operand::Parameter(idx) => {
                // Read from arg registers
                let arg_reg = Register::arg_index(idx as usize);
                self.emitter.mov_reg(reg, arg_reg);
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
            .push_str(&format!("    mov [rbp - {}], {}\n", offset, reg.name()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ir::instr::{BasicBlock, IrFunction, IrModule, IrType};

    #[test]
    fn test_x86_codegen_alloc_and_write_barrier() {
        let mut codegen = IrCodegen::new();
        let module = IrModule {
            globals: vec![],
            functions: vec![IrFunction {
                name: "test_func".to_string(),
                params: vec![],
                return_type: IrType::Void,
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

        // In x86_64, aura_alloc call should use rdi for argument
        assert!(asm.contains("rdi, 16"));
        assert!(asm.contains("call _aura_alloc"));

        assert!(asm.contains("call _aura_write_barrier"));
    }
}
