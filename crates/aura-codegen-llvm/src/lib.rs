use anyhow::Result;
use aura_codegen::{Backend, Target};
use aura_mir::{MirFunction, MirProgram, Statement, Terminator, Lvalue, Rvalue, Operand, Constant};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target as LlvmTarget, TargetMachine, TargetTriple};
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::OptimizationLevel;
use std::collections::HashMap;
use std::path::Path;

pub struct LlvmBackend<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    target_machine: TargetMachine,
}

impl<'ctx> LlvmBackend<'ctx> {
    pub fn new(context: &'ctx Context, name: &str, target: &Target) -> Result<Self> {
        let module = context.create_module(name);
        let builder = context.create_builder();

        LlvmTarget::initialize_all(&InitializationConfig::default());

        let triple = TargetTriple::create(&target.triple);
        let target = LlvmTarget::from_triple(&triple).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let target_machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or_else(|| anyhow::anyhow!("Failed to create target machine"))?;

        module.set_triple(&triple);
        module.set_data_layout(&target_machine.get_target_data().get_data_layout());

        Ok(Self {
            context,
            module,
            builder,
            target_machine,
        })
    }

    fn aura_to_llvm_type(&self, ty: &aura_typeck::Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            aura_typeck::Ty::I32 => self.context.i32_type().as_basic_type_enum(),
            aura_typeck::Ty::I64 => self.context.i64_type().as_basic_type_enum(),
            aura_typeck::Ty::F32 => self.context.f32_type().as_basic_type_enum(),
            aura_typeck::Ty::F64 => self.context.f64_type().as_basic_type_enum(),
            aura_typeck::Ty::Bool => self.context.bool_type().as_basic_type_enum(),
            aura_typeck::Ty::Void => {
                self.context.i8_type().as_basic_type_enum()
            }
            aura_typeck::Ty::String | aura_typeck::Ty::Class(_) => {
                self.context.ptr_type(inkwell::AddressSpace::default()).as_basic_type_enum()
            }
            _ => self.context.i8_type().as_basic_type_enum(),
        }
    }

    fn compile_function(&self, func: &MirFunction) -> Result<FunctionValue<'ctx>> {
        let ret_ty_aura = &func.locals[0].ty;
        let ret_type_llvm = self.aura_to_llvm_type(ret_ty_aura);
        
        let mut arg_types = Vec::new();
        for local in &func.locals {
            if local.kind == aura_mir::LocalKind::Arg {
                arg_types.push(self.aura_to_llvm_type(&local.ty).into());
            }
        }

        let fn_type = if *ret_ty_aura == aura_typeck::Ty::Void {
            self.context.void_type().fn_type(&arg_types, false)
        } else {
            ret_type_llvm.fn_type(&arg_types, false)
        };

        // Reuse if already declared (e.g. built-ins)
        let llvm_func = self.module.get_function(&func.name).unwrap_or_else(|| {
            self.module.add_function(&func.name, fn_type, None)
        });
        
        let mut blocks = HashMap::new();
        for mir_bb in &func.blocks {
            let llvm_bb = self.context.append_basic_block(llvm_func, &format!("bb{}", mir_bb.id));
            blocks.insert(mir_bb.id, llvm_bb);
        }

        let mut locals = HashMap::new();
        self.builder.position_at_end(*blocks.get(&0).unwrap());

        // Allocate all locals
        for (i, local) in func.locals.iter().enumerate() {
            let ty = self.aura_to_llvm_type(&local.ty);
            let ptr = self.builder.build_alloca(ty, local.name.as_deref().unwrap_or(""))?;
            locals.insert(i, ptr);
        }

        // Assign arguments to locals
        let mut arg_idx = 0;
        for (i, local) in func.locals.iter().enumerate() {
            if local.kind == aura_mir::LocalKind::Arg {
                let arg = llvm_func.get_nth_param(arg_idx as u32).unwrap();
                self.builder.build_store(locals[&i], arg)?;
                arg_idx += 1;
            }
        }

        // Lower blocks
        for mir_bb in &func.blocks {
            self.builder.position_at_end(blocks[&mir_bb.id]);
            
            for stmt in &mir_bb.statements {
                match stmt {
                    Statement::Assign(lval, rvalue) => {
                        let val = self.lower_rvalue(rvalue, &locals, func)?;
                        let ptr = self.lower_lvalue(lval, &locals)?;
                        self.builder.build_store(ptr, val)?;
                    }
                }
            }

            if let Some(term) = &mir_bb.terminator {
                match term {
                    Terminator::Goto(id) => {
                        self.builder.build_unconditional_branch(blocks[id])?;
                    }
                    Terminator::Return(val) => {
                        if let Some(v) = val {
                            let llvm_val = self.lower_operand(v, &locals, func)?;
                            self.builder.build_return(Some(&llvm_val))?;
                        } else {
                            self.builder.build_return(None)?;
                        }
                    }
                    Terminator::SwitchInt { discr, targets, otherwise } => {
                        let llvm_discr = self.lower_operand(discr, &locals, func)?.into_int_value();
                        let mut cases = Vec::new();
                        for (val, target) in targets {
                            cases.push((self.context.i64_type().const_int(*val as u64, false), blocks[target]));
                        }
                        self.builder.build_switch(llvm_discr, blocks[otherwise], &cases)?;
                    }
                    Terminator::Call { callee, args, destination, target } => {
                        let callee_name = match callee {
                            Operand::Constant(Constant::String(name)) => name.clone(),
                            _ => "unknown".to_string(),
                        };

                        let llvm_callee = self.get_or_declare_func(&callee_name)?;
                        let mut llvm_args = Vec::new();
                        for arg in args {
                            llvm_args.push(self.lower_operand(arg, &locals, func)?.into());
                        }

                        let call_site = self.builder.build_call(llvm_callee, &llvm_args, "calltmp")?;
                        
                        let dest_ptr = self.lower_lvalue(destination, &locals)?;
                        if let Some(val) = call_site.try_as_basic_value().left() {
                            self.builder.build_store(dest_ptr, val)?;
                        }

                        self.builder.build_unconditional_branch(blocks[target])?;
                    }
                    _ => {
                        self.builder.build_unreachable()?;
                    }
                }
            }
        }

        Ok(llvm_func)
    }

    fn get_or_declare_func(&self, name: &str) -> Result<FunctionValue<'ctx>> {
        if let Some(f) = self.module.get_function(name) {
            return Ok(f);
        }

        // Handle built-ins
        if name == "println" {
            // Map aura println to aura_rt's aura_println
            let rt_name = "aura_println";
            if let Some(f) = self.module.get_function(rt_name) {
                return Ok(f);
            }
            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
            let fn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
            return Ok(self.module.add_function(rt_name, fn_type, None));
        }

        // Generic declaration placeholder
        let fn_type = self.context.void_type().fn_type(&[], false);
        Ok(self.module.add_function(name, fn_type, None))
    }

    fn lower_lvalue(&self, lval: &Lvalue, locals: &HashMap<usize, PointerValue<'ctx>>) -> Result<PointerValue<'ctx>> {
        match lval {
            Lvalue::Local(id) => Ok(locals[id]),
            Lvalue::Field(base, _field_name) => {
                let base_ptr = self.lower_lvalue(base, locals)?;
                Ok(base_ptr)
            }
        }
    }

    fn lower_rvalue(&self, rvalue: &Rvalue, locals: &HashMap<usize, PointerValue<'ctx>>, func_mir: &MirFunction) -> Result<BasicValueEnum<'ctx>> {
        match rvalue {
            Rvalue::Use(op) => self.lower_operand(op, locals, func_mir),
            Rvalue::BinaryOp(op, left, right) => {
                let lop = self.lower_operand(left, locals, func_mir)?;
                let rop = self.lower_operand(right, locals, func_mir)?;
                
                if lop.is_int_value() {
                    let lhs = lop.into_int_value();
                    let rhs = rop.into_int_value();
                    match op {
                        aura_ast::BinaryOp::Add => Ok(self.builder.build_int_add(lhs, rhs, "addtmp")?.into()),
                        aura_ast::BinaryOp::Sub => Ok(self.builder.build_int_sub(lhs, rhs, "subtmp")?.into()),
                        aura_ast::BinaryOp::Mul => Ok(self.builder.build_int_mul(lhs, rhs, "multmp")?.into()),
                        aura_ast::BinaryOp::Div => Ok(self.builder.build_int_signed_div(lhs, rhs, "divtmp")?.into()),
                        aura_ast::BinaryOp::EqEq => Ok(self.builder.build_int_compare(inkwell::IntPredicate::EQ, lhs, rhs, "eqtmp")?.into()),
                        aura_ast::BinaryOp::NotEq => Ok(self.builder.build_int_compare(inkwell::IntPredicate::NE, lhs, rhs, "netmp")?.into()),
                        aura_ast::BinaryOp::Lt => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SLT, lhs, rhs, "lttmp")?.into()),
                        aura_ast::BinaryOp::LtEq => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SLE, lhs, rhs, "ltetmp")?.into()),
                        aura_ast::BinaryOp::Gt => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SGT, lhs, rhs, "gttmp")?.into()),
                        aura_ast::BinaryOp::GtEq => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SGE, lhs, rhs, "gtetmp")?.into()),
                        _ => Ok(lop),
                    }
                } else if lop.is_float_value() {
                    let lhs = lop.into_float_value();
                    let rhs = rop.into_float_value();
                    match op {
                        aura_ast::BinaryOp::Add => Ok(self.builder.build_float_add(lhs, rhs, "faddtmp")?.into()),
                        aura_ast::BinaryOp::Sub => Ok(self.builder.build_float_sub(lhs, rhs, "fsubtmp")?.into()),
                        aura_ast::BinaryOp::Mul => Ok(self.builder.build_float_mul(lhs, rhs, "fmultmp")?.into()),
                        aura_ast::BinaryOp::Div => Ok(self.builder.build_float_div(lhs, rhs, "fdivtmp")?.into()),
                        aura_ast::BinaryOp::EqEq => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OEQ, lhs, rhs, "feqtmp")?.into()),
                        aura_ast::BinaryOp::NotEq => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::UNE, lhs, rhs, "fnetmp")?.into()),
                        aura_ast::BinaryOp::Lt => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OLT, lhs, rhs, "flttmp")?.into()),
                        aura_ast::BinaryOp::LtEq => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OLE, lhs, rhs, "fletmp")?.into()),
                        aura_ast::BinaryOp::Gt => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OGT, lhs, rhs, "fgttmp")?.into()),
                        aura_ast::BinaryOp::GtEq => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OGE, lhs, rhs, "fgetmp")?.into()),
                        _ => Ok(lop),
                    }
                } else {
                    Ok(lop)
                }
            }
            Rvalue::UnaryOp(op, val) => {
                let v = self.lower_operand(val, locals, func_mir)?;
                match op {
                    aura_ast::UnaryOp::Neg => {
                        if v.is_int_value() {
                            Ok(self.builder.build_int_neg(v.into_int_value(), "negtmp")?.into())
                        } else {
                            Ok(self.builder.build_float_neg(v.into_float_value(), "fnegtmp")?.into())
                        }
                    }
                    aura_ast::UnaryOp::Not => {
                        Ok(self.builder.build_not(v.into_int_value(), "nottmp")?.into())
                    }
                }
            }
            _ => {
                Ok(self.context.i64_type().const_int(0, false).into())
            }
        }
    }

    fn lower_operand(&self, op: &Operand, locals: &HashMap<usize, PointerValue<'ctx>>, func_mir: &MirFunction) -> Result<BasicValueEnum<'ctx>> {
        match op {
            Operand::Copy(lval) | Operand::Move(lval) => {
                let ptr = self.lower_lvalue(lval, locals)?;
                let ty_aura = self.get_lvalue_type(lval, func_mir);
                let ty_llvm = self.aura_to_llvm_type(&ty_aura);
                Ok(self.builder.build_load(ty_llvm, ptr, "tmp")?)
            }
            Operand::Constant(c) => {
                match c {
                    Constant::Int(v) => Ok(self.context.i64_type().const_int(*v as u64, false).into()),
                    Constant::Float(v) => Ok(self.context.f64_type().const_float(*v).into()),
                    Constant::Bool(v) => Ok(self.context.bool_type().const_int(*v as u64, false).into()),
                    Constant::String(s) => {
                        // For MVP strings, call aura_string_new_utf8
                        let rt_new_str = self.get_or_declare_string_new_utf8()?;
                        
                        let global_str = self.builder.build_global_string_ptr(s, "str_lit")?;
                        let len = self.context.i64_type().const_int(s.len() as u64, false);
                        
                        let call = self.builder.build_call(rt_new_str, &[global_str.as_basic_value_enum().into(), len.into()], "str_new")?;
                        Ok(call.try_as_basic_value().left().unwrap())
                    }
                }
            }
        }
    }

    fn get_or_declare_string_new_utf8(&self) -> Result<FunctionValue<'ctx>> {
        let name = "aura_string_new_utf8";
        if let Some(f) = self.module.get_function(name) {
            return Ok(f);
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let len_type = self.context.i64_type();
        let fn_type = ptr_type.fn_type(&[ptr_type.into(), len_type.into()], false);
        Ok(self.module.add_function(name, fn_type, None))
    }

    fn get_lvalue_type(&self, lval: &Lvalue, func: &MirFunction) -> aura_typeck::Ty {
        match lval {
            Lvalue::Local(id) => func.locals[*id].ty.clone(),
            Lvalue::Field(base, _name) => {
                self.get_lvalue_type(base, func)
            }
        }
    }
}

impl<'ctx> Backend for LlvmBackend<'ctx> {
    fn compile(&self, program: &MirProgram, out_dir: &Path) -> Result<String> {
        for func in &program.functions {
            self.compile_function(func)?;
        }

        let obj_path = out_dir.join("main.o").to_str().unwrap().to_string();
        self.target_machine
            .write_to_file(&self.module, FileType::Object, Path::new(&obj_path))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        Ok(obj_path)
    }

    fn emit_llvm(&self, _program: &MirProgram, out_path: &Path) -> Result<()> {
        self.module.print_to_file(out_path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }

    fn emit_asm(&self, _program: &MirProgram, out_path: &Path) -> Result<()> {
        self.target_machine
            .write_to_file(&self.module, FileType::Assembly, out_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}
