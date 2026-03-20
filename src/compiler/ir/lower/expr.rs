use super::Lowerer;
use crate::compiler::ast::Expr;
use crate::compiler::ir::instr::{Instruction, Operand};
use crate::compiler::sema::ty::Type;

impl Lowerer {
    pub fn lower_expr(&mut self, expr: Expr) -> Operand {
        match expr {
            Expr::Number(n, _) => {
                self.last_expr_ty = Type::Int32;
                Operand::Constant(n as i64)
            }
            Expr::Float(f, _) => {
                self.last_expr_ty = Type::Float64;
                Operand::FloatingConstant(f)
            }
            Expr::Null(_) => {
                self.last_expr_ty = Type::Null;
                Operand::Constant(0)
            }
            Expr::Template(parts, _) => {
                let mut res_op: Option<Operand> = None;
                for part in parts {
                    let part_op = match part {
                        crate::compiler::ast::TemplatePart::Str(s) => {
                            let name = format!("str_{}", self.globals.len());
                            self.globals.push((name.clone(), s));
                            self.builder.call(
                                "aura_get_string".to_string(),
                                vec![Operand::Constant(self.globals.len() as i64 - 1)],
                            )
                        }
                        crate::compiler::ast::TemplatePart::Expr(expr) => {
                            let op = self.lower_expr(*expr);
                            let ty = self.last_expr_ty.clone();
                            if ty.is_float() {
                                self.builder
                                    .fcall("aura_float_to_str".to_string(), vec![op])
                            } else if ty.is_integer() {
                                self.builder.call("aura_num_to_str".to_string(), vec![op])
                            } else if matches!(ty, Type::String) {
                                op
                            } else if matches!(ty, Type::Boolean) {
                                self.builder.call("aura_bool_to_str".to_string(), vec![op])
                            } else {
                                op
                            }
                        }
                    };

                    res_op = match res_op {
                        Some(prev) => Some(
                            self.builder
                                .call("aura_str_concat".to_string(), vec![prev, part_op]),
                        ),
                        None => Some(part_op),
                    };
                }
                self.last_expr_ty = Type::String;
                res_op.expect("Empty template literal")
            }
            Expr::Await(_, _) => todo!("Await in IR lower"),
            Expr::ArrayLiteral(elements, _) => {
                let len = elements.len();
                let arr_ptr = self.builder.call(
                    "aura_array_new".to_string(),
                    vec![Operand::Constant(len as i64), Operand::Constant(0)],
                );
                for el in elements {
                    let el_op = self.lower_expr(el);
                    self.builder
                        .call("aura_array_push".to_string(), vec![arr_ptr.clone(), el_op]);
                }
                self.last_expr_ty = Type::Array(Box::new(Type::Error));
                arr_ptr
            }
            Expr::Error(_) => panic!("Compiler bug: error node in IR lower"),
            Expr::StringLiteral(s, _) => {
                let name = format!("str_{}", self.globals.len());
                self.globals.push((name.clone(), s));
                self.last_expr_ty = Type::String;
                // Treat strings as pointers in IR for now
                self.builder.call(
                    "aura_get_string".to_string(),
                    vec![Operand::Constant(self.globals.len() as i64 - 1)],
                )
            }
            Expr::Variable(name, _) => {
                if let Some((ptr_reg, cls_name, ty)) = self.mem_vars.get(&name).cloned() {
                    self.last_expr_ty = ty.clone().unwrap_or_else(|| {
                        if let Some(cls) = cls_name {
                            Type::Class(cls)
                        } else {
                            Type::Error
                        }
                    });
                    let is_union = matches!(ty, Some(Type::Union(_)));
                    let offset = if is_union { 8 } else { 0 };
                    let dest = self.builder.new_reg();
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Load(
                            dest,
                            Operand::Value(ptr_reg),
                            offset,
                        ));
                    Operand::Value(dest)
                } else if self.class_structures.contains_key(&name) {
                    self.last_expr_ty = Type::Class(name);
                    Operand::Constant(0) // Class constant is 0
                } else if self.enums.contains_key(&name) {
                    self.last_expr_ty = Type::Enum(name);
                    Operand::Constant(0) // Enum constant is 0
                } else {
                    panic!("Undefined variable {}", name);
                }
            }
            Expr::BinaryOp(left, op, right, _) => {
                let mut lhs = self.lower_expr(*left);
                let lhs_ty = self.last_expr_ty.clone();
                let mut rhs = self.lower_expr(*right);
                let rhs_ty = self.last_expr_ty.clone();

                let is_float = lhs_ty.is_float() || rhs_ty.is_float();

                // Numeric promotion
                if is_float {
                    if lhs_ty.is_integer() {
                        lhs = self.builder.itof(lhs);
                    }
                    if rhs_ty.is_integer() {
                        rhs = self.builder.itof(rhs);
                    }
                }

                let op_str = op.as_str();

                self.last_expr_ty = if op == "=="
                    || op == "!="
                    || op == "<"
                    || op == "<="
                    || op == ">"
                    || op == ">="
                {
                    Type::Boolean
                } else {
                    if is_float {
                        Type::Float64
                    } else {
                        lhs_ty
                    }
                };

                if is_float {
                    match op_str {
                        "+" => self.builder.fadd(lhs, rhs),
                        "-" => self.builder.fsub(lhs, rhs),
                        "*" => self.builder.fmul(lhs, rhs),
                        "/" => self.builder.fdiv(lhs, rhs),
                        "%" => self.builder.frem(lhs, rhs),
                        "==" => self.builder.feq(lhs, rhs),
                        "!=" => self.builder.fne(lhs, rhs),
                        "<" => self.builder.flt(lhs, rhs),
                        "<=" => self.builder.fle(lhs, rhs),
                        ">" => self.builder.fgt(lhs, rhs),
                        ">=" => self.builder.fge(lhs, rhs),
                        _ => panic!("Unsupported float operator {}", op),
                    }
                } else {
                    match op_str {
                        "+" => self.builder.add(lhs, rhs),
                        "-" => self.builder.sub(lhs, rhs),
                        "*" => self.builder.mul(lhs, rhs),
                        "/" => self.builder.div(lhs, rhs),
                        "%" => self.builder.rem(lhs, rhs),
                        "==" => self.builder.eq(lhs, rhs),
                        "!=" => self.builder.ne(lhs, rhs),
                        "<" => self.builder.lt(lhs, rhs),
                        "<=" => self.builder.le(lhs, rhs),
                        ">" => self.builder.gt(lhs, rhs),
                        ">=" => self.builder.ge(lhs, rhs),
                        "&" => self.builder.bit_and(lhs, rhs),
                        "|" => self.builder.bit_or(lhs, rhs),
                        "^" => self.builder.bit_xor(lhs, rhs),
                        "<<" => self.builder.shl(lhs, rhs),
                        ">>" => self.builder.shr(lhs, rhs),
                        _ => panic!("Unsupported operator {}", op),
                    }
                }
            }
            Expr::Call(name, _type_args, _name_span, args, _span) => {
                let (p_tys, r_ty) = self
                    .function_tys
                    .get(&name)
                    .cloned()
                    .unwrap_or((vec![], Type::Error));
                let mut ir_args = Vec::new();
                for (i, a) in args.into_iter().enumerate() {
                    let mut op = self.lower_expr(a);
                    let arg_ty = self.last_expr_ty.clone();

                    if let Some(pty) = p_tys.get(i) {
                        if let Type::Union(_) = pty {
                            // If the argument is already a union, decompose it
                            if let Type::Union(_) = arg_ty {
                                // For now, we only handle union variables as arguments
                                // Complex union expressions would need a temp slot
                                // But they are likely variables anyway
                            }
                            // Always promote to union (tag, value)
                            let tag = arg_ty.tag();
                            ir_args.push(Operand::Constant(tag));
                            ir_args.push(op);
                            continue;
                        }

                        if pty.is_float() && arg_ty.is_integer() {
                            op = self.builder.itof(op);
                        }
                    }
                    ir_args.push(op);
                }
                self.last_expr_ty = r_ty;
                self.builder.call(name, ir_args)
            }
            Expr::Assign(name, value, _) => {
                let mut val_op = self.lower_expr(*value);
                let (ptr_reg, _, v_ty) = self
                    .mem_vars
                    .get(&name)
                    .cloned()
                    .expect("Undefined variable");

                let source_ty = self.last_expr_ty.clone();

                if let Some(target_ty) = v_ty {
                    if let Type::Union(_) = target_ty {
                        // Store tag at offset 0 and value at offset 8
                        let tag = source_ty.tag();
                        self.builder
                            .store(Operand::Constant(tag), Operand::Value(ptr_reg), 0);
                        self.builder
                            .store(val_op.clone(), Operand::Value(ptr_reg), 8);
                    } else {
                        if target_ty.is_float() && source_ty.is_integer() {
                            val_op = self.builder.itof(val_op);
                        }
                        self.builder
                            .emit(crate::compiler::ir::instr::Instruction::Store(
                                val_op.clone(),
                                Operand::Value(ptr_reg),
                                0,
                            ));
                    }
                }
                val_op
            }
            Expr::New(class_name, _type_args, _name_span, args, _span) => {
                let size = self
                    .class_layouts
                    .get(&class_name)
                    .map(|l| l.size)
                    .unwrap_or(0);
                let obj_reg = self.builder.call(
                    "aura_alloc".to_string(),
                    vec![Operand::Constant(size as i64)],
                );
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::SetVTable(
                        obj_reg.clone(),
                        class_name.clone(),
                    ));

                // Call constructor if exists
                let ctor_name = format!("{}_ctor", class_name);
                let (p_tys, _) = self
                    .function_tys
                    .get(&ctor_name)
                    .cloned()
                    .unwrap_or((vec![], Type::Error));

                let mut ctor_args = vec![obj_reg.clone()];
                for (i, a) in args.into_iter().enumerate() {
                    let mut op = self.lower_expr(a);
                    let arg_ty = self.last_expr_ty.clone();
                    // constructor p_tys[0] is 'this'
                    if let Some(pty) = p_tys.get(i + 1) {
                        if let Type::Union(_) = pty {
                            ctor_args.push(Operand::Constant(arg_ty.tag()));
                            ctor_args.push(op);
                            continue;
                        }
                        if pty.is_float() && arg_ty.is_integer() {
                            op = self.builder.itof(op);
                        }
                    }
                    ctor_args.push(op);
                }
                self.builder.call(ctor_name, ctor_args);

                self.last_expr_ty = Type::Class(class_name);
                obj_reg
            }
            Expr::This(_) => {
                let (ptr_reg, _, _) = self
                    .mem_vars
                    .get("this")
                    .cloned()
                    .expect("this not in scope");
                self.last_expr_ty = self
                    .current_class
                    .as_ref()
                    .map(|c| Type::Class(c.clone()))
                    .unwrap_or(Type::Error);
                let dest = self.builder.new_reg();
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Load(
                        dest,
                        Operand::Value(ptr_reg),
                        0,
                    ));
                Operand::Value(dest)
            }
            Expr::MemberAccess(obj, field, _, _) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();

                if let Type::Enum(enum_name) = obj_ty {
                    if let Some(members) = self.enums.get(&enum_name) {
                        if let Some((op, ty)) = members.get(&field) {
                            self.last_expr_ty = ty.clone();
                            if ty == &Type::String {
                                return self
                                    .builder
                                    .call("aura_get_string".to_string(), vec![op.clone()]);
                            } else {
                                return op.clone();
                            }
                        }
                    }
                    panic!("Enum field not found in IR: {}.{}", enum_name, field);
                }

                let cls_name = match &obj_ty {
                    Type::Class(cls) => Some(cls.clone()),
                    Type::Union(ref options) => {
                        let mut first_cls = None;
                        for opt in options {
                            if let Type::Class(cls) = opt {
                                first_cls = Some(cls.clone());
                                break;
                            }
                        }
                        first_cls
                    }
                    _ => None,
                };

                if let Some(cls_name) = cls_name {
                    let offset = self.get_field_offset(&cls_name, &field);
                    let actual_field_ty = self
                        .class_structures
                        .get(&cls_name)
                        .and_then(|s| s.get(&field))
                        .cloned()
                        .unwrap_or(Type::Error);
                    let dest = self.builder.new_reg();
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Load(
                            dest, obj_op, offset,
                        ));
                    self.last_expr_ty = actual_field_ty;
                    Operand::Value(dest)
                } else {
                    panic!("Member access on non-class type {:?}", obj_ty);
                }
            }
            Expr::MemberAssign(obj, field, value, _, _) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();
                let mut val_op = self.lower_expr(*value);
                let val_ty = self.last_expr_ty.clone();

                let cls_name = match &obj_ty {
                    Type::Class(cls) => Some(cls.clone()),
                    Type::Union(ref options) => {
                        let mut first_cls = None;
                        for opt in options {
                            if let Type::Class(cls) = opt {
                                first_cls = Some(cls.clone());
                                break;
                            }
                        }
                        first_cls
                    }
                    _ => None,
                };

                if let Some(cls_name) = cls_name {
                    let offset = self.get_field_offset(&cls_name, &field);
                    let field_ty = self
                        .class_structures
                        .get(&cls_name)
                        .and_then(|s| s.get(&field))
                        .cloned()
                        .unwrap_or(Type::Error);

                    if field_ty.is_float() && val_ty.is_integer() {
                        val_op = self.builder.itof(val_op);
                    }

                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            val_op.clone(),
                            obj_op.clone(),
                            offset,
                        ));

                    if field_ty.is_class() || field_ty.is_array() {
                        self.builder
                            .emit(crate::compiler::ir::instr::Instruction::WriteBarrier(
                                obj_op,
                                val_op.clone(),
                            ));
                    }
                    val_op
                } else {
                    panic!("Member assign on non-class type {:?}", obj_ty);
                }
            }
            Expr::MethodCall(obj, method, _type_args, _name_span, args, _span) => {
                let obj_op = self.lower_expr(*obj.clone());
                let obj_ty = self.last_expr_ty.clone();

                if let Expr::Super(_) = *obj {
                    // super.method(...) is a static call to parent's method
                    if let Some(cls_name) = &self.current_class {
                        if let Some(parent) = self.parent_classes.get(cls_name) {
                            let mangled_name = format!("{}_{}", parent, method);
                            let mut ir_args = vec![obj_op]; // obj_op is 'this' for super

                            // Lookup parent method's p_tys
                            let (p_tys, r_ty) = self
                                .function_tys
                                .get(&mangled_name)
                                .cloned()
                                .unwrap_or((vec![], Type::Error));

                            for (i, a) in args.into_iter().enumerate() {
                                let mut op = self.lower_expr(a);
                                let arg_ty = self.last_expr_ty.clone();
                                if let Some(pty) = p_tys.get(i + 1) {
                                    // +1 for 'this'
                                    if let Type::Union(_) = pty {
                                        ir_args.push(Operand::Constant(arg_ty.tag()));
                                        ir_args.push(op);
                                        continue;
                                    }
                                    if pty.is_float() && arg_ty.is_integer() {
                                        op = self.builder.itof(op);
                                    }
                                }
                                ir_args.push(op);
                            }
                            self.last_expr_ty = r_ty;
                            return self.builder.call(mangled_name, ir_args);
                        }
                    }
                }

                let mut is_static = false;
                let mut vtable_idx = None;
                let mut return_ty = Type::Error;

                let mangled_name = match &obj_ty {
                    Type::Class(ref cls_name) => {
                        if let Some(statics) = self.static_methods.get(cls_name) {
                            if statics.contains(&method) {
                                is_static = true;
                            }
                        }
                        if !is_static {
                            if let Some(layout) = self.class_layouts.get(cls_name) {
                                vtable_idx = layout.vtable_index.get(&method).cloned();
                            }
                        }

                        // Try to find return type
                        if let Some((_, r_ty)) =
                            self.function_tys.get(&format!("{}_{}", cls_name, method))
                        {
                            return_ty = r_ty.clone();
                        }

                        format!("{}_{}", cls_name, method)
                    }
                    Type::Union(ref options) => {
                        // Find a class in the union to get the mangled name for signature lookup
                        let mut first_cls = None;
                        for opt in options {
                            if let Type::Class(cls) = opt {
                                first_cls = Some(cls.clone());
                                break;
                            }
                        }

                        if let Some(cls_name) = first_cls {
                            // Assuming all classes in the union have the same vtable structure for this method
                            if let Some(layout) = self.class_layouts.get(&cls_name) {
                                vtable_idx = layout.vtable_index.get(&method).cloned();
                            }
                            if let Some((_, r_ty)) =
                                self.function_tys.get(&format!("{}_{}", cls_name, method))
                            {
                                return_ty = r_ty.clone();
                            }
                            format!("{}_{}", cls_name, method)
                        } else {
                            method.clone()
                        }
                    }
                    _ => method.clone(),
                };

                let (p_tys, _) = self
                    .function_tys
                    .get(&mangled_name)
                    .cloned()
                    .unwrap_or((vec![], Type::Error));

                let mut ir_args = Vec::new();
                if !is_static {
                    ir_args.push(obj_op.clone());
                }

                for (i, a) in args.into_iter().enumerate() {
                    let mut op = self.lower_expr(a);
                    let arg_ty = self.last_expr_ty.clone();
                    // For instance methods, p_tys[0] is 'this', so we check p_tys[i+1]
                    // For static methods, p_tys[i] is correct
                    let p_idx = if is_static { i } else { i + 1 };
                    if let Some(pty) = p_tys.get(p_idx) {
                        if let Type::Union(_) = pty {
                            ir_args.push(Operand::Constant(arg_ty.tag()));
                            ir_args.push(op);
                            continue;
                        }
                        if pty.is_float() && arg_ty.is_integer() {
                            op = self.builder.itof(op);
                        }
                    }
                    ir_args.push(op);
                }
                self.last_expr_ty = return_ty;

                if let Some(idx) = vtable_idx {
                    self.builder.call_virtual(obj_op, idx, ir_args)
                } else if let Some(global_idx) = self.method_to_idx.get(&method) {
                    // Fallback to global method index for interface/abstract calls
                    self.builder.call_virtual(obj_op, *global_idx, ir_args)
                } else {
                    self.builder.call(mangled_name, ir_args)
                }
            }
            Expr::Super(_) => {
                // super as a value is just 'this' cast to parent type
                let (ptr_reg, _, _) = self
                    .mem_vars
                    .get("this")
                    .cloned()
                    .expect("this not in scope");
                self.last_expr_ty = self
                    .current_class
                    .as_ref()
                    .and_then(|c| self.parent_classes.get(c))
                    .map(|p| Type::Class(p.clone()))
                    .unwrap_or(Type::Error);
                let dest = self.builder.new_reg();
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Load(
                        dest,
                        Operand::Value(ptr_reg),
                        0,
                    ));
                Operand::Value(dest)
            }
            Expr::SuperCall(args, _) => {
                // super(...) in constructor
                let (ptr_reg, _, _) = self
                    .mem_vars
                    .get("this")
                    .cloned()
                    .expect("this not in scope");
                let this_op = {
                    let dest = self.builder.new_reg();
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Load(
                            dest,
                            Operand::Value(ptr_reg),
                            0,
                        ));
                    Operand::Value(dest)
                };

                if let Some(cls_name) = &self.current_class {
                    if let Some(parent) = self.parent_classes.get(cls_name) {
                        let ctor_name = format!("{}_ctor", parent);
                        let (p_tys, _) = self
                            .function_tys
                            .get(&ctor_name)
                            .cloned()
                            .unwrap_or((vec![], Type::Error));

                        let mut ir_args = vec![this_op];
                        for (i, a) in args.into_iter().enumerate() {
                            let mut op = self.lower_expr(a);
                            if let Some(pty) = p_tys.get(i + 1) {
                                if pty.is_float() && self.last_expr_ty.is_integer() {
                                    op = self.builder.itof(op);
                                }
                            }
                            ir_args.push(op);
                        }
                        self.builder.call(ctor_name, ir_args);
                    }
                }
                self.last_expr_ty = Type::Void;
                Operand::Constant(0)
            }
            Expr::UnaryOp(op, expr, _) => {
                let val = self.lower_expr(*expr);
                if op == "-" {
                    self.builder.sub(Operand::Constant(0), val)
                } else if op == "~" {
                    self.builder.bit_not(val)
                } else {
                    val
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                let target_ty = self.resolve_type(ty_expr);

                if let Type::Class(name) = &target_ty {
                    let val_op = self.lower_expr(*expr);
                    let vtable_addr = self.builder.new_reg();
                    self.builder
                        .emit(Instruction::LoadVTableAddress(vtable_addr, name.clone()));
                    let res = self.builder.call(
                        "aura_check_class".to_string(),
                        vec![val_op, Operand::Value(vtable_addr)],
                    );
                    self.last_expr_ty = Type::Boolean;
                    res
                } else {
                    let type_tag = target_ty.tag();

                    // If the source is a union variable, we need to load its tag from offset 0
                    let tag_op = if let Expr::Variable(name, _) = &*expr {
                        if let Some((ptr_reg, _, Some(Type::Union(_)))) = self.mem_vars.get(name) {
                            self.builder.load(Operand::Value(*ptr_reg), 0)
                        } else {
                            // If it's not a union, lower it and use its static tag
                            self.lower_expr(*expr);
                            Operand::Constant(self.last_expr_ty.tag())
                        }
                    } else {
                        self.lower_expr(*expr);
                        Operand::Constant(self.last_expr_ty.tag())
                    };

                    let res = self.builder.call(
                        "aura_check_tag".to_string(),
                        vec![tag_op, Operand::Constant(type_tag)],
                    );
                    self.last_expr_ty = Type::Boolean;
                    res
                }
            }
            Expr::Throw(expr, _) => {
                let val = self.lower_expr(*expr);
                self.builder.call("aura_throw".to_string(), vec![val]);
                Operand::Constant(0)
            }
            Expr::Index(obj, index, _) => {
                let obj_op = self.lower_expr(*obj);
                let index_op = self.lower_expr(*index);
                // Lower to intrinsic call for now
                self.builder
                    .call("__arr_get".to_string(), vec![obj_op, index_op])
            }
            Expr::IndexAssign(obj, index, value, _) => {
                let obj_op = self.lower_expr(*obj);
                let index_op = self.lower_expr(*index);
                let val_op = self.lower_expr(*value);
                self.builder.call(
                    "aura_array_set".to_string(),
                    vec![obj_op, index_op, val_op.clone()],
                );
                val_op
            }
        }
    }
}
