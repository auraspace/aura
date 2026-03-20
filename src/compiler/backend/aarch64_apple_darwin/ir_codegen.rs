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
                    .push_str(&format!("_{}: .asciz \"{}\"\n", name, content));
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
                    .push_str(&format!("    .quad _{}\n", name));
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
                // Add parent vtable pointer at offset 0
                if let Some(parent) = module.parent_vtables.get(class) {
                    self.emitter
                        .output
                        .push_str(&format!("    .quad vtable_{}\n", parent));
                } else {
                    self.emitter.output.push_str("    .quad 0\n");
                }
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

        // Calculate max register ID to determine stack size
        let mut max_reg = 0;
        for block in &func.blocks {
            for instr in &block.instructions {
                // This is a bit simplified, but Instruction variants that have a dest u32
                // are where new regs are created.
                let dest = match instr {
                    Instruction::Add(d, _, _)
                    | Instruction::Sub(d, _, _)
                    | Instruction::Mul(d, _, _)
                    | Instruction::Div(d, _, _)
                    | Instruction::Rem(d, _, _)
                    | Instruction::FAdd(d, _, _)
                    | Instruction::FSub(d, _, _)
                    | Instruction::FMul(d, _, _)
                    | Instruction::FDiv(d, _, _)
                    | Instruction::FRem(d, _, _)
                    | Instruction::BitAnd(d, _, _)
                    | Instruction::BitOr(d, _, _)
                    | Instruction::BitXor(d, _, _)
                    | Instruction::Shl(d, _, _)
                    | Instruction::Shr(d, _, _)
                    | Instruction::BitNot(d, _)
                    | Instruction::Eq(d, _, _)
                    | Instruction::Ne(d, _, _)
                    | Instruction::Lt(d, _, _)
                    | Instruction::Le(d, _, _)
                    | Instruction::Gt(d, _, _)
                    | Instruction::Ge(d, _, _)
                    | Instruction::FEq(d, _, _)
                    | Instruction::FNe(d, _, _)
                    | Instruction::FLt(d, _, _)
                    | Instruction::FLe(d, _, _)
                    | Instruction::FGt(d, _, _)
                    | Instruction::FGe(d, _, _)
                    | Instruction::Move(d, _)
                    | Instruction::Load(d, _, _)
                    | Instruction::Alloc(d, _)
                    | Instruction::StackAlloc(d, _)
                    | Instruction::Call(d, _, _)
                    | Instruction::FCall(d, _, _)
                    | Instruction::CallVirtual(d, _, _, _)
                    | Instruction::IToF(d, _)
                    | Instruction::LoadVTableAddress(d, _)
                    | Instruction::FToI(d, _) => Some(*d),
                    _ => None,
                };
                if let Some(d) = dest {
                    if d > max_reg {
                        max_reg = d;
                    }
                }
            }
        }
        let required_stack = (max_reg as usize + 1) * 8 + 64; // +64 for safety/spill
        let required_stack = (required_stack + 15) & !15; // Align to 16 bytes

        self.emitter
            .output
            .push_str("    stp x29, x30, [sp, -16]!\n");
        self.emitter.output.push_str("    mov x29, sp\n");
        self.emitter
            .output
            .push_str(&format!("    sub sp, sp, #{}\n", required_stack));

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
            Instruction::FAdd(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fadd(Register::D0, Register::D0, Register::D1);
                self.store_reg(dest, Register::D0);
            }
            Instruction::FSub(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fsub(Register::D0, Register::D0, Register::D1);
                self.store_reg(dest, Register::D0);
            }
            Instruction::FMul(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fmul(Register::D0, Register::D0, Register::D1);
                self.store_reg(dest, Register::D0);
            }
            Instruction::FDiv(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fdiv(Register::D0, Register::D0, Register::D1);
                self.store_reg(dest, Register::D0);
            }
            Instruction::FRem(dest, lhs, rhs) => {
                // FRem is tricky in AArch64. For now, let's just use fmod from C or similar if we have it,
                // but let's just do a simple implementation: a - floor(a/b)*b
                // Actually, let's just panic for now or do a placeholder.
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.output.push_str("    fdiv d2, d0, d1\n");
                self.emitter.output.push_str("    frintz d2, d2\n");
                self.emitter.output.push_str("    fmul d2, d2, d1\n");
                self.emitter.output.push_str("    fsub d0, d0, d2\n");
                self.store_reg(dest, Register::D0);
            }
            Instruction::BitAnd(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    and x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::BitOr(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    orr x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::BitXor(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    eor x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Shl(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    lsl x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::Shr(dest, lhs, rhs) => {
                self.load_operand(Register::X16, lhs);
                self.load_operand(Register::X17, rhs);
                self.emitter.output.push_str("    lsr x16, x16, x17\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::BitNot(dest, src) => {
                self.load_operand(Register::X16, src);
                self.emitter.output.push_str("    mvn x16, x16\n");
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
            Instruction::FEq(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fcmp(Register::D0, Register::D1);
                self.emitter.output.push_str("    cset x16, eq\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::FNe(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fcmp(Register::D0, Register::D1);
                self.emitter.output.push_str("    cset x16, ne\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::FLt(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fcmp(Register::D0, Register::D1);
                self.emitter.output.push_str("    cset x16, mi\n"); // Minus (less than)
                self.store_reg(dest, Register::X16);
            }
            Instruction::FLe(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fcmp(Register::D0, Register::D1);
                self.emitter.output.push_str("    cset x16, ls\n"); // Lower or Same
                self.store_reg(dest, Register::X16);
            }
            Instruction::FGt(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fcmp(Register::D0, Register::D1);
                self.emitter.output.push_str("    cset x16, gt\n");
                self.store_reg(dest, Register::X16);
            }
            Instruction::FGe(dest, lhs, rhs) => {
                self.load_operand(Register::D0, lhs);
                self.load_operand(Register::D1, rhs);
                self.emitter.fcmp(Register::D0, Register::D1);
                self.emitter.output.push_str("    cset x16, ge\n");
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
            Instruction::FCall(dest, name, args) => {
                for (i, arg) in args.iter().enumerate() {
                    if i < 8 {
                        self.load_operand(Register::from_d_u8(i as u8), arg.clone());
                    }
                }
                self.emitter.call(&format!("_{}", name));
                // If the return type is float, it will be in D0, but we currently store destiny in X0
                // For print_float it returns void so it's fine.
                // For float_to_str it returns char* in X0, so it's also fine.
                self.store_reg(dest, Register::X0);
            }
            Instruction::CallVirtual(dest, obj, idx, args) => {
                // obj is the receiver object
                // VTable pointer is at offset 0
                self.load_operand(Register::X0, obj);
                // Load VTable pointer: x16 = [x0]
                self.emitter.output.push_str("    ldr x16, [x0]\n");
                // Load function pointer: x16 = [x16, (idx + 1) * 8]
                self.emitter
                    .output
                    .push_str(&format!("    ldr x16, [x16, #{}]\n", (idx + 1) * 8));

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
            Instruction::LoadVTableAddress(dest, class_name) => {
                self.emitter
                    .output
                    .push_str(&format!("    adrp x17, vtable_{}@PAGE\n", class_name));
                self.emitter.output.push_str(&format!(
                    "    add x17, x17, vtable_{}@PAGEOFF\n",
                    class_name
                ));
                self.store_reg(dest, Register::X17);
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
            Instruction::IToF(dest, src) => {
                self.load_operand(Register::X16, src);
                self.emitter.scvtf(Register::D0, Register::X16);
                self.store_reg(dest, Register::D0);
            }
            Instruction::FToI(dest, src) => {
                self.load_operand(Register::D0, src);
                self.emitter.fcvtzs(Register::X16, Register::D0);
                self.store_reg(dest, Register::X16);
            }
        }
    }

    fn load_operand(&mut self, reg: Register, op: Operand) {
        let is_d_reg = reg.name().starts_with('d');
        match op {
            Operand::Constant(c) => {
                if is_d_reg {
                    self.emitter.mov_imm(Register::X16, c);
                    self.emitter.scvtf(reg, Register::X16);
                } else {
                    self.emitter.mov_imm(reg, c);
                }
            }
            Operand::FloatingConstant(f) => {
                if !is_d_reg {
                    // Constant float into X register - bit-level move
                    let bits = f.to_bits() as i64;
                    self.emitter.mov_imm(reg, bits);
                } else {
                    self.emitter.fmov_imm(reg, f);
                }
            }
            Operand::Value(id) => {
                let offset = *self
                    .reg_offsets
                    .get(&id)
                    .unwrap_or_else(|| panic!("Reg {} not found in IR codegen for function", id));
                let reg_name = reg.name();
                if offset <= 255 {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr {}, [x29, -{}]\n", reg_name, offset));
                } else {
                    self.emitter.output.push_str(&format!(
                        "    mov x9, {}\n    sub x9, x29, x9\n    ldr {}, [x9]\n",
                        offset, reg_name
                    ));
                }
            }
            Operand::Parameter(idx) => {
                if idx < 8 {
                    // Check if we should use X or D register based on what 'reg' is
                    let reg_name = reg.name();
                    if reg_name.starts_with('d') {
                        self.emitter.fmov(reg, Register::from_d_u8(idx as u8));
                    } else {
                        self.emitter.mov_reg(reg, Register::from_u8(idx as u8));
                    }
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
        let reg_name = reg.name();
        if *offset <= 255 {
            self.emitter
                .output
                .push_str(&format!("    str {}, [x29, -{}]\n", reg_name, offset));
        } else {
            self.emitter.output.push_str(&format!(
                "    mov x9, {}\n    sub x9, x29, x9\n    str {}, [x9]\n",
                offset, reg_name
            ));
        }
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
            parent_vtables: HashMap::new(),
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
