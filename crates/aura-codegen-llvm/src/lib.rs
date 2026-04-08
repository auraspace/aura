use anyhow::Result;
use aura_codegen::{Backend, Target};
use aura_mir::{Constant, Lvalue, MirFunction, MirProgram, Operand, Rvalue, Statement, Terminator};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target as LlvmTarget, TargetMachine,
    TargetTriple,
};
use inkwell::types::{BasicType, BasicTypeEnum, StructType};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, GlobalValue, PointerValue};
use inkwell::OptimizationLevel;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct LlvmBackend<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    target_machine: TargetMachine,
    class_structs: RefCell<HashMap<String, inkwell::types::StructType<'ctx>>>,
    class_field_order: RefCell<HashMap<String, Vec<String>>>,
    class_field_types: RefCell<HashMap<String, HashMap<String, aura_typeck::Ty>>>,
    class_vtables: RefCell<HashMap<String, GlobalValue<'ctx>>>,
    method_slots: RefCell<Vec<String>>,
}

impl<'ctx> LlvmBackend<'ctx> {
    pub fn new(context: &'ctx Context, name: &str, target: &Target) -> Result<Self> {
        let module = context.create_module(name);
        let builder = context.create_builder();

        LlvmTarget::initialize_all(&InitializationConfig::default());

        let triple = TargetTriple::create(&target.triple);
        let target =
            LlvmTarget::from_triple(&triple).map_err(|e| anyhow::anyhow!(e.to_string()))?;
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
            class_structs: RefCell::new(HashMap::new()),
            class_field_order: RefCell::new(HashMap::new()),
            class_field_types: RefCell::new(HashMap::new()),
            class_vtables: RefCell::new(HashMap::new()),
            method_slots: RefCell::new(Vec::new()),
        })
    }

    fn aura_to_llvm_type(&self, ty: &aura_typeck::Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            aura_typeck::Ty::I32 => self.context.i32_type().as_basic_type_enum(),
            aura_typeck::Ty::I64 => self.context.i64_type().as_basic_type_enum(),
            aura_typeck::Ty::F32 => self.context.f32_type().as_basic_type_enum(),
            aura_typeck::Ty::F64 => self.context.f64_type().as_basic_type_enum(),
            aura_typeck::Ty::Bool => self.context.bool_type().as_basic_type_enum(),
            aura_typeck::Ty::Void => self.context.i8_type().as_basic_type_enum(),
            aura_typeck::Ty::String | aura_typeck::Ty::Class(_) | aura_typeck::Ty::Interface(_) => {
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .as_basic_type_enum()
            }
            _ => self.context.i64_type().as_basic_type_enum(),
        }
    }

    fn prepare_class_layouts(&self, program: &MirProgram) {
        self.class_structs.borrow_mut().clear();
        self.class_field_order.borrow_mut().clear();
        self.class_field_types.borrow_mut().clear();
        self.class_vtables.borrow_mut().clear();
        self.method_slots.borrow_mut().clear();

        self.method_slots
            .borrow_mut()
            .extend(program.method_slots.iter().cloned());

        for (class_name, class) in &program.classes {
            self.class_field_order
                .borrow_mut()
                .insert(class_name.clone(), class.field_order.clone());
            self.class_field_types
                .borrow_mut()
                .insert(class_name.clone(), class.fields.clone());

            let struct_ty = self.context.opaque_struct_type(class_name);
            self.class_structs
                .borrow_mut()
                .insert(class_name.clone(), struct_ty);
        }

        for (class_name, class) in &program.classes {
            let mut field_types: Vec<BasicTypeEnum<'ctx>> = Vec::new();
            field_types.push(
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .into(),
            );
            field_types.push(self.context.i64_type().into());
            for field_name in &class.field_order {
                if let Some(field_ty) = class.fields.get(field_name) {
                    field_types.push(self.aura_to_llvm_type(field_ty));
                }
            }
            if let Some(struct_ty) = self.class_structs.borrow().get(class_name) {
                struct_ty.set_body(&field_types, false);
            }
        }
    }

    fn object_header_type(&self) -> StructType<'ctx> {
        self.context.struct_type(
            &[
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .into(),
                self.context.i64_type().into(),
            ],
            false,
        )
    }

    fn vtable_struct_type(&self) -> StructType<'ctx> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let slot_types: Vec<_> = self
            .method_slots
            .borrow()
            .iter()
            .map(|_| ptr_ty.into())
            .collect();
        self.context.struct_type(&slot_types, false)
    }

    fn method_slot_index(&self, method_name: &str) -> Option<u32> {
        self.method_slots
            .borrow()
            .iter()
            .position(|name| name == method_name)
            .map(|idx| idx as u32)
    }

    fn method_signature_from_receiver(
        &self,
        program: &MirProgram,
        receiver_ty: &aura_typeck::Ty,
        method_name: &str,
    ) -> Option<aura_typeck::MethodSig> {
        match receiver_ty {
            aura_typeck::Ty::Class(class_name) => {
                self.resolve_class_method_signature(program, class_name, method_name)
            }
            aura_typeck::Ty::Interface(interface_name) => program
                .interfaces
                .get(interface_name)
                .and_then(|iface| iface.methods.get(method_name))
                .cloned(),
            _ => None,
        }
    }

    fn resolve_class_method_signature(
        &self,
        program: &MirProgram,
        class_name: &str,
        method_name: &str,
    ) -> Option<aura_typeck::MethodSig> {
        if let Some(class) = program.classes.get(class_name) {
            if let Some(func) = class.methods.get(method_name) {
                return Some(aura_typeck::MethodSig {
                    params: func
                        .locals
                        .iter()
                        .filter(|local| local.kind == aura_mir::LocalKind::Arg)
                        .skip(1)
                        .map(|local| local.ty.clone())
                        .collect(),
                    return_ty: func.locals[0].ty.clone(),
                });
            }
            if let Some(parent) = &class.extends {
                return self.resolve_class_method_signature(program, parent, method_name);
            }
        }
        None
    }

    fn resolve_method_function<'a>(
        &self,
        program: &'a MirProgram,
        class_name: &str,
        method_name: &str,
    ) -> Option<&'a MirFunction> {
        let class = program.classes.get(class_name)?;
        if let Some(func) = class.methods.get(method_name) {
            return Some(func);
        }
        class
            .extends
            .as_deref()
            .and_then(|parent| self.resolve_method_function(program, parent, method_name))
    }

    fn build_vtables(&self, program: &MirProgram) -> Result<()> {
        let vtable_ty = self.vtable_struct_type();
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let method_slots = self.method_slots.borrow().clone();

        for class_name in program.classes.keys() {
            let mut entries = Vec::new();
            for slot_name in &method_slots {
                if let Some(func) = self.resolve_method_function(program, class_name, slot_name) {
                    let llvm_fn = self
                        .module
                        .get_function(&func.name)
                        .ok_or_else(|| anyhow::anyhow!("missing llvm function `{}`", func.name))?;
                    entries.push(llvm_fn.as_global_value().as_pointer_value().into());
                } else {
                    entries.push(ptr_ty.const_null().into());
                }
            }
            let initializer = vtable_ty.const_named_struct(&entries);
            let global = self
                .module
                .add_global(vtable_ty, None, &format!("{class_name}.vtable"));
            global.set_initializer(&initializer);
            global.set_constant(true);
            self.class_vtables
                .borrow_mut()
                .insert(class_name.clone(), global);
        }

        Ok(())
    }

    fn class_struct_type(&self, class_name: &str) -> Option<inkwell::types::StructType<'ctx>> {
        self.class_structs.borrow().get(class_name).copied()
    }

    fn class_field_index(&self, class_name: &str, field_name: &str) -> Option<u32> {
        self.class_field_order
            .borrow()
            .get(class_name)
            .and_then(|fields| fields.iter().position(|f| f == field_name))
            .map(|idx| idx as u32 + 2)
    }

    fn class_field_type(&self, class_name: &str, field_name: &str) -> Option<aura_typeck::Ty> {
        self.class_field_types
            .borrow()
            .get(class_name)
            .and_then(|fields| fields.get(field_name))
            .cloned()
    }

    fn declare_function(&self, func: &MirFunction) -> Result<FunctionValue<'ctx>> {
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

        Ok(self
            .module
            .get_function(&func.name)
            .unwrap_or_else(|| self.module.add_function(&func.name, fn_type, None)))
    }

    fn compile_function(
        &self,
        program: &MirProgram,
        func: &MirFunction,
    ) -> Result<FunctionValue<'ctx>> {
        let llvm_func = self.declare_function(func)?;

        let mut blocks = HashMap::new();
        for mir_bb in &func.blocks {
            let llvm_bb = self
                .context
                .append_basic_block(llvm_func, &format!("bb{}", mir_bb.id));
            blocks.insert(mir_bb.id, llvm_bb);
        }

        let mut locals = HashMap::new();
        self.builder.position_at_end(*blocks.get(&0).unwrap());

        // Allocate all locals
        for (i, local) in func.locals.iter().enumerate() {
            let ty = self.aura_to_llvm_type(&local.ty);
            let ptr = self
                .builder
                .build_alloca(ty, local.name.as_deref().unwrap_or(""))?;
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
                        let val = self.lower_rvalue(program, rvalue, &locals, func)?;
                        let ptr = self.lower_lvalue(lval, &locals, func)?;
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
                        if func.locals[0].ty == aura_typeck::Ty::Void {
                            self.builder.build_return(None)?;
                        } else if let Some(v) = val {
                            let llvm_val = self.lower_operand(program, v, &locals, func)?;
                            self.builder.build_return(Some(&llvm_val))?;
                        } else {
                            self.builder.build_return(None)?;
                        }
                    }
                    Terminator::SwitchInt {
                        discr,
                        targets,
                        otherwise,
                    } => {
                        let llvm_discr = self
                            .lower_operand(program, discr, &locals, func)?
                            .into_int_value();
                        let mut cases = Vec::new();
                        for (val, target) in targets {
                            cases.push((
                                self.context.i64_type().const_int(*val as u64, false),
                                blocks[target],
                            ));
                        }
                        self.builder
                            .build_switch(llvm_discr, blocks[otherwise], &cases)?;
                    }
                    Terminator::Call {
                        callee,
                        args,
                        destination,
                        target,
                    } => {
                        let callee_name = match callee {
                            Operand::Constant(Constant::String(name)) => name.clone(),
                            _ => "unknown".to_string(),
                        };

                        if let Some(class_name) = callee_name.strip_prefix("new:") {
                            let obj_ptr = self.allocate_class_instance(class_name)?;
                            let dest_ptr = self.lower_lvalue(destination, &locals, func)?;
                            self.builder.build_store(dest_ptr, obj_ptr)?;
                            self.builder.build_unconditional_branch(blocks[target])?;
                            continue;
                        }

                        if let Some(method_name) = callee_name.strip_prefix("method:") {
                            let receiver_op = args.first().ok_or_else(|| {
                                anyhow::anyhow!("method call missing receiver operand")
                            })?;
                            let receiver_ty = self
                                .operand_type(receiver_op, func)
                                .unwrap_or(aura_typeck::Ty::Unknown);
                            let sig = self
                                .method_signature_from_receiver(program, &receiver_ty, method_name)
                                .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "unable to resolve method `{method_name}` for `{}`",
                                        receiver_ty.name()
                                    )
                                })?;

                            let receiver_val =
                                self.lower_operand(program, receiver_op, &locals, func)?;
                            let receiver_ptr = receiver_val.into_pointer_value();
                            let slot_idx =
                                self.method_slot_index(method_name).ok_or_else(|| {
                                    anyhow::anyhow!("unknown method slot `{method_name}`")
                                })?;
                            let header_ty = self.object_header_type();
                            let header_ptr_ty =
                                header_ty.ptr_type(inkwell::AddressSpace::default());
                            let header_ptr = self.builder.build_pointer_cast(
                                receiver_ptr,
                                header_ptr_ty,
                                "dispatch_header",
                            )?;
                            let vtable_ptr_ptr = self.builder.build_struct_gep(
                                header_ty,
                                header_ptr,
                                0,
                                "dispatch_vtable_ptr",
                            )?;
                            let vtable_ptr = self.builder.build_load(
                                self.context.ptr_type(inkwell::AddressSpace::default()),
                                vtable_ptr_ptr,
                                "dispatch_vtable",
                            )?;
                            let vtable_ty = self.vtable_struct_type();
                            let vtable_ptr_ty =
                                vtable_ty.ptr_type(inkwell::AddressSpace::default());
                            let vtable_struct_ptr = self.builder.build_pointer_cast(
                                vtable_ptr.into_pointer_value(),
                                vtable_ptr_ty,
                                "dispatch_vtable_cast",
                            )?;
                            let slot_ptr = self.builder.build_struct_gep(
                                vtable_ty,
                                vtable_struct_ptr,
                                slot_idx,
                                "dispatch_slot_ptr",
                            )?;
                            let fn_ptr = self.builder.build_load(
                                self.context.ptr_type(inkwell::AddressSpace::default()),
                                slot_ptr,
                                "dispatch_fn",
                            )?;

                            let fn_ty = self.method_llvm_fn_type(&receiver_ty, &sig);
                            let mut llvm_args = Vec::new();
                            let mut receiver_arg =
                                self.lower_operand(program, receiver_op, &locals, func)?;
                            if let Some(first_param_ty) = fn_ty.get_param_types().first() {
                                if receiver_arg.get_type() != *first_param_ty {
                                    if receiver_arg.is_int_value() && first_param_ty.is_int_type() {
                                        receiver_arg = self
                                            .builder
                                            .build_int_cast(
                                                receiver_arg.into_int_value(),
                                                first_param_ty.into_int_type(),
                                                "receiver_cast",
                                            )?
                                            .into();
                                    }
                                }
                            }
                            llvm_args.push(receiver_arg.into());

                            for (i, arg) in args.iter().enumerate().skip(1) {
                                let mut val = self.lower_operand(program, arg, &locals, func)?;
                                if let Some(param_ty) = fn_ty.get_param_types().get(i) {
                                    if val.get_type() != *param_ty {
                                        if val.is_int_value() && param_ty.is_int_type() {
                                            val = self
                                                .builder
                                                .build_int_cast(
                                                    val.into_int_value(),
                                                    param_ty.into_int_type(),
                                                    "arg_cast",
                                                )?
                                                .into();
                                        }
                                    }
                                }
                                llvm_args.push(val.into());
                            }

                            let call_site = self.builder.build_indirect_call(
                                fn_ty,
                                fn_ptr.into_pointer_value(),
                                &llvm_args,
                                "calltmp",
                            )?;

                            let dest_ptr = self.lower_lvalue(destination, &locals, func)?;
                            if let Some(val) = call_site.try_as_basic_value().left() {
                                self.builder.build_store(dest_ptr, val)?;
                            }

                            self.builder.build_unconditional_branch(blocks[target])?;
                            continue;
                        }

                        let llvm_callee = self.get_or_declare_func(&callee_name)?;
                        let param_types = llvm_callee.get_type().get_param_types();
                        let mut llvm_args = Vec::new();
                        for (i, arg) in args.iter().enumerate() {
                            let mut val = self.lower_operand(program, arg, &locals, func)?;
                            if let Some(param_ty) = param_types.get(i) {
                                if val.get_type() != *param_ty {
                                    if val.is_int_value() && param_ty.is_int_type() {
                                        val = self
                                            .builder
                                            .build_int_cast(
                                                val.into_int_value(),
                                                param_ty.into_int_type(),
                                                "arg_cast",
                                            )?
                                            .into();
                                    }
                                }
                            }
                            llvm_args.push(val.into());
                        }

                        let call_site =
                            self.builder
                                .build_call(llvm_callee, &llvm_args, "calltmp")?;

                        let dest_ptr = self.lower_lvalue(destination, &locals, func)?;
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

    fn allocate_class_instance(&self, class_name: &str) -> Result<PointerValue<'ctx>> {
        let struct_ty = self
            .class_struct_type(class_name)
            .ok_or_else(|| anyhow::anyhow!("unknown class `{class_name}`"))?;
        let size = self
            .target_machine
            .get_target_data()
            .get_store_size(&struct_ty);
        let align = self
            .target_machine
            .get_target_data()
            .get_abi_alignment(&struct_ty);

        let alloc = self.get_or_declare_alloc()?;
        let size_val = self.context.i64_type().const_int(size, false);
        let align_val = self.context.i64_type().const_int(align as u64, false);
        let call =
            self.builder
                .build_call(alloc, &[size_val.into(), align_val.into()], "obj_alloc")?;
        let raw_ptr = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| anyhow::anyhow!("allocator did not return a pointer"))?
            .into_pointer_value();
        let obj_ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let obj_ptr = self
            .builder
            .build_pointer_cast(raw_ptr, obj_ptr_ty, "obj_cast")?;

        let header_ty = self.object_header_type();
        let header_ptr_ty = header_ty.ptr_type(inkwell::AddressSpace::default());
        let header_ptr = self
            .builder
            .build_pointer_cast(obj_ptr, header_ptr_ty, "header_cast")?;
        let header_vtable_ptr =
            self.builder
                .build_struct_gep(header_ty, header_ptr, 0, "header_vtable")?;
        let header_ref_ptr =
            self.builder
                .build_struct_gep(header_ty, header_ptr, 1, "header_refcount")?;
        let vtable_global = self
            .class_vtables
            .borrow()
            .get(class_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing vtable for class `{class_name}`"))?;
        self.builder
            .build_store(header_vtable_ptr, vtable_global.as_pointer_value())?;
        self.builder
            .build_store(header_ref_ptr, self.context.i64_type().const_int(1, false))?;

        Ok(obj_ptr)
    }

    fn get_or_declare_alloc(&self) -> Result<FunctionValue<'ctx>> {
        if let Some(f) = self.module.get_function("aura_alloc") {
            return Ok(f);
        }
        let i8_ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = i8_ptr.fn_type(
            &[
                self.context.i64_type().into(),
                self.context.i64_type().into(),
            ],
            false,
        );
        Ok(self.module.add_function("aura_alloc", fn_type, None))
    }

    fn get_or_declare_func(&self, name: &str) -> Result<FunctionValue<'ctx>> {
        if let Some(f) = self.module.get_function(name) {
            return Ok(f);
        }

        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());

        // Handle built-ins and runtime helpers
        match name {
            "println" => {
                let rt_name = "aura_println";
                if let Some(f) = self.module.get_function(rt_name) {
                    return Ok(f);
                }
                let fn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
                Ok(self.module.add_function(rt_name, fn_type, None))
            }
            "aura_i32_to_string" => {
                let fn_type = ptr_type.fn_type(&[self.context.i32_type().into()], false);
                Ok(self.module.add_function(name, fn_type, None))
            }
            "aura_i64_to_string" => {
                let fn_type = ptr_type.fn_type(&[self.context.i64_type().into()], false);
                Ok(self.module.add_function(name, fn_type, None))
            }
            "aura_f32_to_string" => {
                let fn_type = ptr_type.fn_type(&[self.context.f32_type().into()], false);
                Ok(self.module.add_function(name, fn_type, None))
            }
            "aura_f64_to_string" => {
                let fn_type = ptr_type.fn_type(&[self.context.f64_type().into()], false);
                Ok(self.module.add_function(name, fn_type, None))
            }
            "aura_bool_to_string" => {
                let fn_type = ptr_type.fn_type(&[self.context.bool_type().into()], false);
                Ok(self.module.add_function(name, fn_type, None))
            }
            _ => {
                // Generic declaration placeholder
                let fn_type = self.context.void_type().fn_type(&[], false);
                Ok(self.module.add_function(name, fn_type, None))
            }
        }
    }

    fn lower_lvalue(
        &self,
        lval: &Lvalue,
        locals: &HashMap<usize, PointerValue<'ctx>>,
        func: &MirFunction,
    ) -> Result<PointerValue<'ctx>> {
        match lval {
            Lvalue::Local(id) => Ok(locals[id]),
            Lvalue::Field(base, field_name) => {
                let base_ty = self.get_lvalue_type(base, func);
                let class_name = match base_ty {
                    aura_typeck::Ty::Class(name) => name,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "field access is only supported on class instances"
                        ))
                    }
                };

                let obj_ptr = self.load_class_object_ptr(base, locals, func)?;
                let field_idx =
                    self.class_field_index(&class_name, field_name)
                        .ok_or_else(|| {
                            anyhow::anyhow!("unknown field `{field_name}` on class `{class_name}`")
                        })?;

                let struct_ty = self
                    .class_struct_type(&class_name)
                    .ok_or_else(|| anyhow::anyhow!("unknown class `{class_name}`"))?;
                let field_ptr =
                    self.builder
                        .build_struct_gep(struct_ty, obj_ptr, field_idx, "field_ptr")?;
                Ok(field_ptr)
            }
        }
    }

    fn load_class_object_ptr(
        &self,
        lval: &Lvalue,
        locals: &HashMap<usize, PointerValue<'ctx>>,
        func: &MirFunction,
    ) -> Result<PointerValue<'ctx>> {
        let lval_ty = self.get_lvalue_type(lval, func);
        match lval_ty {
            aura_typeck::Ty::Class(_) => {
                let slot_ptr = self.lower_lvalue(lval, locals, func)?;
                let loaded = self.builder.build_load(
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    slot_ptr,
                    "objptr",
                )?;
                Ok(loaded.into_pointer_value())
            }
            _ => self.lower_lvalue(lval, locals, func),
        }
    }

    fn lower_rvalue(
        &self,
        program: &MirProgram,
        rvalue: &Rvalue,
        locals: &HashMap<usize, PointerValue<'ctx>>,
        func_mir: &MirFunction,
    ) -> Result<BasicValueEnum<'ctx>> {
        match rvalue {
            Rvalue::Use(op) => self.lower_operand(program, op, locals, func_mir),
            Rvalue::BinaryOp(op, left, right) => {
                let lop = self.lower_operand(program, left, locals, func_mir)?;
                let rop = self.lower_operand(program, right, locals, func_mir)?;

                if lop.is_int_value() {
                    let lhs = lop.into_int_value();
                    let rhs = rop.into_int_value();
                    match op {
                        aura_ast::BinaryOp::Add => {
                            Ok(self.builder.build_int_add(lhs, rhs, "addtmp")?.into())
                        }
                        aura_ast::BinaryOp::Sub => {
                            Ok(self.builder.build_int_sub(lhs, rhs, "subtmp")?.into())
                        }
                        aura_ast::BinaryOp::Mul => {
                            Ok(self.builder.build_int_mul(lhs, rhs, "multmp")?.into())
                        }
                        aura_ast::BinaryOp::Div => Ok(self
                            .builder
                            .build_int_signed_div(lhs, rhs, "divtmp")?
                            .into()),
                        aura_ast::BinaryOp::EqEq => Ok(self
                            .builder
                            .build_int_compare(inkwell::IntPredicate::EQ, lhs, rhs, "eqtmp")?
                            .into()),
                        aura_ast::BinaryOp::NotEq => Ok(self
                            .builder
                            .build_int_compare(inkwell::IntPredicate::NE, lhs, rhs, "netmp")?
                            .into()),
                        aura_ast::BinaryOp::Lt => Ok(self
                            .builder
                            .build_int_compare(inkwell::IntPredicate::SLT, lhs, rhs, "lttmp")?
                            .into()),
                        aura_ast::BinaryOp::LtEq => Ok(self
                            .builder
                            .build_int_compare(inkwell::IntPredicate::SLE, lhs, rhs, "ltetmp")?
                            .into()),
                        aura_ast::BinaryOp::Gt => Ok(self
                            .builder
                            .build_int_compare(inkwell::IntPredicate::SGT, lhs, rhs, "gttmp")?
                            .into()),
                        aura_ast::BinaryOp::GtEq => Ok(self
                            .builder
                            .build_int_compare(inkwell::IntPredicate::SGE, lhs, rhs, "gtetmp")?
                            .into()),
                        _ => Ok(lop),
                    }
                } else if lop.is_float_value() {
                    let lhs = lop.into_float_value();
                    let rhs = rop.into_float_value();
                    match op {
                        aura_ast::BinaryOp::Add => {
                            Ok(self.builder.build_float_add(lhs, rhs, "faddtmp")?.into())
                        }
                        aura_ast::BinaryOp::Sub => {
                            Ok(self.builder.build_float_sub(lhs, rhs, "fsubtmp")?.into())
                        }
                        aura_ast::BinaryOp::Mul => {
                            Ok(self.builder.build_float_mul(lhs, rhs, "fmultmp")?.into())
                        }
                        aura_ast::BinaryOp::Div => {
                            Ok(self.builder.build_float_div(lhs, rhs, "fdivtmp")?.into())
                        }
                        aura_ast::BinaryOp::EqEq => Ok(self
                            .builder
                            .build_float_compare(inkwell::FloatPredicate::OEQ, lhs, rhs, "feqtmp")?
                            .into()),
                        aura_ast::BinaryOp::NotEq => Ok(self
                            .builder
                            .build_float_compare(inkwell::FloatPredicate::UNE, lhs, rhs, "fnetmp")?
                            .into()),
                        aura_ast::BinaryOp::Lt => Ok(self
                            .builder
                            .build_float_compare(inkwell::FloatPredicate::OLT, lhs, rhs, "flttmp")?
                            .into()),
                        aura_ast::BinaryOp::LtEq => Ok(self
                            .builder
                            .build_float_compare(inkwell::FloatPredicate::OLE, lhs, rhs, "fletmp")?
                            .into()),
                        aura_ast::BinaryOp::Gt => Ok(self
                            .builder
                            .build_float_compare(inkwell::FloatPredicate::OGT, lhs, rhs, "fgttmp")?
                            .into()),
                        aura_ast::BinaryOp::GtEq => Ok(self
                            .builder
                            .build_float_compare(inkwell::FloatPredicate::OGE, lhs, rhs, "fgetmp")?
                            .into()),
                        _ => Ok(lop),
                    }
                } else {
                    Ok(lop)
                }
            }
            Rvalue::UnaryOp(op, val) => {
                let v = self.lower_operand(program, val, locals, func_mir)?;
                match op {
                    aura_ast::UnaryOp::Neg => {
                        if v.is_int_value() {
                            Ok(self
                                .builder
                                .build_int_neg(v.into_int_value(), "negtmp")?
                                .into())
                        } else {
                            Ok(self
                                .builder
                                .build_float_neg(v.into_float_value(), "fnegtmp")?
                                .into())
                        }
                    }
                    aura_ast::UnaryOp::Not => {
                        Ok(self.builder.build_not(v.into_int_value(), "nottmp")?.into())
                    }
                }
            }
            _ => Ok(self.context.i64_type().const_int(0, false).into()),
        }
    }

    fn lower_operand(
        &self,
        _program: &MirProgram,
        op: &Operand,
        locals: &HashMap<usize, PointerValue<'ctx>>,
        func_mir: &MirFunction,
    ) -> Result<BasicValueEnum<'ctx>> {
        match op {
            Operand::Copy(lval) | Operand::Move(lval) => {
                let ptr = self.lower_lvalue(lval, locals, func_mir)?;
                let ty_aura = self.get_lvalue_type(lval, func_mir);
                let ty_llvm = self.aura_to_llvm_type(&ty_aura);
                Ok(self.builder.build_load(ty_llvm, ptr, "tmp")?)
            }
            Operand::Constant(c) => {
                match c {
                    Constant::Int(v) => {
                        Ok(self.context.i64_type().const_int(*v as u64, false).into())
                    }
                    Constant::Float(v) => Ok(self.context.f64_type().const_float(*v).into()),
                    Constant::Bool(v) => {
                        Ok(self.context.bool_type().const_int(*v as u64, false).into())
                    }
                    Constant::String(s) => {
                        // For MVP strings, call aura_string_new_utf8
                        let rt_new_str = self.get_or_declare_string_new_utf8()?;

                        let global_str = self.builder.build_global_string_ptr(s, "str_lit")?;
                        let len = self.context.i64_type().const_int(s.len() as u64, false);

                        let call = self.builder.build_call(
                            rt_new_str,
                            &[global_str.as_basic_value_enum().into(), len.into()],
                            "str_new",
                        )?;
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
            Lvalue::Field(base, name) => {
                let base_ty = self.get_lvalue_type(base, func);
                match base_ty {
                    aura_typeck::Ty::Class(ref class_name) => self
                        .class_field_type(class_name, name)
                        .unwrap_or(aura_typeck::Ty::Unknown),
                    _ => aura_typeck::Ty::Unknown,
                }
            }
        }
    }

    fn operand_type(&self, op: &Operand, func: &MirFunction) -> Option<aura_typeck::Ty> {
        match op {
            Operand::Copy(lval) | Operand::Move(lval) => Some(self.get_lvalue_type(lval, func)),
            Operand::Constant(Constant::String(_)) => Some(aura_typeck::Ty::String),
            Operand::Constant(Constant::Int(_)) => Some(aura_typeck::Ty::I64),
            Operand::Constant(Constant::Float(_)) => Some(aura_typeck::Ty::F64),
            Operand::Constant(Constant::Bool(_)) => Some(aura_typeck::Ty::Bool),
        }
    }

    fn method_llvm_fn_type(
        &self,
        receiver_ty: &aura_typeck::Ty,
        sig: &aura_typeck::MethodSig,
    ) -> inkwell::types::FunctionType<'ctx> {
        let mut arg_types = Vec::new();
        arg_types.push(self.aura_to_llvm_type(receiver_ty).into());
        for param in &sig.params {
            arg_types.push(self.aura_to_llvm_type(param).into());
        }
        let ret_ty = self.aura_to_llvm_type(&sig.return_ty);
        if sig.return_ty == aura_typeck::Ty::Void {
            self.context.void_type().fn_type(&arg_types, false)
        } else {
            ret_ty.fn_type(&arg_types, false)
        }
    }
}

impl<'ctx> Backend for LlvmBackend<'ctx> {
    fn compile(&self, program: &MirProgram, out_dir: &Path) -> Result<PathBuf> {
        self.prepare_class_layouts(program);

        for func in &program.functions {
            self.declare_function(func)?;
        }
        for class in program.classes.values() {
            for method in class.methods.values() {
                self.declare_function(method)?;
            }
        }

        self.build_vtables(program)?;

        for func in &program.functions {
            self.compile_function(program, func)?;
        }

        for class in program.classes.values() {
            for method in class.methods.values() {
                self.compile_function(program, method)?;
            }
        }

        let obj_path = out_dir.join("main.o");
        self.target_machine
            .write_to_file(&self.module, FileType::Object, &obj_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        Ok(obj_path)
    }

    fn emit_llvm(&self, _program: &MirProgram, out_path: &Path) -> Result<()> {
        self.module
            .print_to_file(out_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }

    fn emit_asm(&self, _program: &MirProgram, out_path: &Path) -> Result<()> {
        self.target_machine
            .write_to_file(&self.module, FileType::Assembly, out_path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}
