use super::Codegen;
use crate::compiler::ast::{Expr, TemplatePart};
use crate::compiler::backend::aarch64_apple_darwin::asm::Register as AsmRegister;
use crate::compiler::sema::ty::Type;

impl Codegen {
    pub fn generate_expr(&mut self, expr: Expr) {
        match expr {
            Expr::Number(val, _) => {
                self.emitter.mov_imm(AsmRegister::X0, val);
            }
            Expr::Null(_) => {
                self.emitter.mov_imm(AsmRegister::X0, 0);
            }
            Expr::StringLiteral(val, _) => {
                let label = if let Some(l) = self.string_constants.get(&val) {
                    l.clone()
                } else {
                    let l = self.new_label("str");
                    self.string_constants.insert(val, l.clone());
                    l
                };
                self.emitter
                    .output
                    .push_str(&format!("    adrp x0, {}@PAGE\n", label));
                self.emitter
                    .output
                    .push_str(&format!("    add x0, x0, {}@PAGEOFF\n", label));
            }
            Expr::Variable(name, _) => {
                if let Some((offset, _)) = self.variables.get(&name) {
                    self.load_local("x0", *offset);
                } else if let Some((label, _)) = self.global_variables.get(&name) {
                    self.emitter
                        .output
                        .push_str(&format!("    adrp x1, {}@PAGE\n", label));
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x0, [x1, {}@PAGEOFF]\n", label));
                } else if self.classes.contains_key(&name) {
                    self.emitter.mov_imm(AsmRegister::X0, 0); // Class reference is null for now
                } else {
                    match name.as_str() {
                        "true" => self.emitter.mov_imm(AsmRegister::X0, 1),
                        "false" => self.emitter.mov_imm(AsmRegister::X0, 0),
                        "null" => self.emitter.mov_imm(AsmRegister::X0, 0),
                        "O_RDONLY" => self.emitter.mov_imm(AsmRegister::X0, 0),
                        "O_WRONLY" => self.emitter.mov_imm(AsmRegister::X0, 1),
                        "O_RDWR" => self.emitter.mov_imm(AsmRegister::X0, 2),
                        "O_CREAT" => self.emitter.mov_imm(AsmRegister::X0, 512),
                        "O_TRUNC" => self.emitter.mov_imm(AsmRegister::X0, 1024),
                        "O_APPEND" => self.emitter.mov_imm(AsmRegister::X0, 8),
                        _ => panic!("Undefined variable {}", name),
                    }
                }
            }
            Expr::BinaryOp(left, op, right, _) => {
                let left_ty = self.get_node_type(&left.span()).cloned();
                let right_ty = self.get_node_type(&right.span()).cloned();

                // String concatenation
                let mut is_string_concat = false;
                if let (Some(lty), Some(rty)) = (&left_ty, &right_ty) {
                    if (matches!(lty, Type::String) || matches!(rty, Type::String)) && op == "+" {
                        is_string_concat = true;
                    }
                }
                if !is_string_concat && op == "+" {
                    if matches!(&*left, Expr::StringLiteral(_, _))
                        || matches!(&*right, Expr::StringLiteral(_, _))
                    {
                        is_string_concat = true;
                    }
                }

                self.generate_expr(*right);
                self.emitter.push(AsmRegister::X0);
                self.generate_expr(*left);
                self.emitter.pop(AsmRegister::X1);

                if is_string_concat {
                    self.emitter.call("_aura_str_concat");
                    return;
                }

                match op.as_str() {
                    "+" => self
                        .emitter
                        .add(AsmRegister::X0, AsmRegister::X0, AsmRegister::X1),
                    "-" => self
                        .emitter
                        .sub(AsmRegister::X0, AsmRegister::X0, AsmRegister::X1),
                    "*" => self
                        .emitter
                        .mul(AsmRegister::X0, AsmRegister::X0, AsmRegister::X1),
                    "/" => self
                        .emitter
                        .sdiv(AsmRegister::X0, AsmRegister::X0, AsmRegister::X1),
                    "%" => {
                        self.emitter
                            .sdiv(AsmRegister::X2, AsmRegister::X0, AsmRegister::X1); // X2 = a / b
                        self.emitter
                            .mul(AsmRegister::X2, AsmRegister::X2, AsmRegister::X1); // X2 = (a / b) * b
                        self.emitter
                            .sub(AsmRegister::X0, AsmRegister::X0, AsmRegister::X2);
                        // X0 = a - X2
                    }
                    "==" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, eq\n");
                    }
                    "!=" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, ne\n");
                    }
                    "<" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, lt\n");
                    }
                    "<=" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, le\n");
                    }
                    ">" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, gt\n");
                    }
                    ">=" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, ge\n");
                    }
                    "&&" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, #0\n    cset x0, ne\n");
                        self.emitter
                            .output
                            .push_str("    cmp x1, #0\n    cset x1, ne\n");
                        self.emitter.output.push_str("    and x0, x0, x1\n");
                    }
                    "||" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, #0\n    cset x0, ne\n");
                        self.emitter
                            .output
                            .push_str("    cmp x1, #0\n    cset x1, ne\n");
                        self.emitter.output.push_str("    orr x0, x0, x1\n");
                    }
                    "|" => {
                        self.emitter.output.push_str("    orr x0, x0, x1\n");
                    }
                    "&" => {
                        self.emitter.output.push_str("    and x0, x0, x1\n");
                    }
                    _ => panic!("Unsupported operator {}", op),
                }
            }
            Expr::Assign(name, value, _) => {
                self.generate_expr(*value);
                if let Some((offset, _)) = self.variables.get(&name) {
                    self.store_local("x0", *offset);
                } else if let Some((label, _)) = self.global_variables.get(&name) {
                    self.emitter.push(AsmRegister::X0); // Save value
                    self.emitter
                        .output
                        .push_str(&format!("    adrp x1, {}@PAGE\n", label));
                    self.emitter.pop(AsmRegister::X0); // Restore value
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x1, {}@PAGEOFF]\n", label));
                } else {
                    panic!("Undefined variable {}", name);
                }
            }
            Expr::This(_) => {
                let (offset, _) = self
                    .variables
                    .get("this")
                    .expect("'this' used outside of method");
                self.load_local("x0", *offset);
            }
            Expr::New(class_name, _, args, _) => {
                let (fields, _) = self
                    .classes
                    .get(&class_name)
                    .expect(&format!("Undefined class {}", class_name));
                let size = (fields.len() + 1) * 8; // +1 for VTable pointer at offset 0
                self.emitter.mov_imm(AsmRegister::X0, size as i64);
                self.emitter.call("_aura_alloc");

                // Set VTable at offset 0
                self.emitter
                    .output
                    .push_str(&format!("    adrp x1, vtable_{}@PAGE\n", class_name));
                self.emitter
                    .output
                    .push_str(&format!("    add x1, x1, vtable_{}@PAGEOFF\n", class_name));
                self.emitter.output.push_str("    str x1, [x0]\n");

                // Push result (instance) to save it while evaluating args
                self.emitter.push(AsmRegister::X0);

                // Now push instance as the first argument ('this')
                self.emitter.push(AsmRegister::X0);

                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(AsmRegister::X0);
                }

                // Pop args into x0-x7
                let num_args = args.len() + 1; // +1 for 'this'
                for i in (0..num_args.min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }

                self.emitter.call(&format!("_{}_ctor", class_name));

                self.emitter.output.push_str("    ldr x0, [sp], 16\n");
            }
            Expr::MemberAccess(obj, member, _, _span) => {
                if let Expr::Variable(ref name, _) = *obj {
                    if let Some(enum_def) = self.enums.get(name) {
                        if let Some(val_expr) = enum_def.get(&member) {
                            self.generate_expr(val_expr.clone());
                            return;
                        }
                    }
                }

                let mut offset = 0;
                let ty = self.resolve_obj_type(&obj);

                if let Type::Class(ref class_name) = ty {
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = (idx + 1) * 8;
                        }
                    }
                } else if let Some(ref class_name) = self.current_class {
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = (idx + 1) * 8;
                        }
                    }
                }

                self.generate_expr(*obj);
                self.emitter
                    .output
                    .push_str(&format!("    ldr x0, [x0, #{}]\n", offset));
            }
            Expr::MemberAssign(obj, member, value, _, _span) => {
                let ty = self.resolve_obj_type(&obj);
                let mut offset = 0;
                if let Type::Class(ref class_name) = ty {
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = (idx + 1) * 8;
                        }
                    }
                }

                self.generate_expr(*value);
                self.emitter.push(AsmRegister::X0);
                self.generate_expr(*obj);
                self.emitter.pop(AsmRegister::X1);
                self.emitter
                    .output
                    .push_str(&format!("    str x1, [x0, #{}]\n", offset));
                self.emitter.mov_reg(AsmRegister::X0, AsmRegister::X1); // Assignment result
            }
            Expr::MethodCall(obj, member, _, args, _span) => {
                let obj_span = obj.span();
                let mut is_static = false;
                let mut class_name_found = None;
                let mut is_primitive = false;

                if let Expr::Variable(ref name, _) = *obj {
                    if self.classes.contains_key(name) {
                        is_static = true;
                        class_name_found = Some(name.clone());
                    }
                }

                if !is_static {
                    let mut ty = self
                        .get_node_type(&obj_span)
                        .cloned()
                        .unwrap_or(Type::Unknown);

                    if matches!(ty, Type::Unknown) {
                        match &*obj {
                            Expr::StringLiteral(_, _) => ty = Type::String,
                            Expr::ArrayLiteral(_, _) => ty = Type::Array(Box::new(Type::Unknown)),
                            Expr::Variable(ref name, _) => {
                                if let Some((_, var_ty)) = self.variables.get(name) {
                                    ty = var_ty.clone();
                                } else if let Some((_, var_ty)) = self.global_variables.get(name) {
                                    ty = var_ty.clone();
                                }
                            }
                            _ => {}
                        }
                    }

                    if matches!(ty, Type::Unknown) {
                        if matches!(
                            member.as_str(),
                            "charAt"
                                | "substring"
                                | "indexOf"
                                | "toUpper"
                                | "toLower"
                                | "trim"
                                | "len"
                        ) {
                            ty = Type::String;
                        } else if matches!(member.as_str(), "push" | "pop" | "join" | "get" | "len")
                        {
                            ty = Type::Array(Box::new(Type::Unknown));
                        }
                    }

                    if matches!(ty, Type::String | Type::Array(_)) {
                        is_primitive = true;
                    }
                }

                if is_static {
                    self.emitter.mov_imm(AsmRegister::X0, 0); // dummy this
                    self.emitter.push(AsmRegister::X0);
                } else if is_primitive {
                    // Object IS the this for primitives
                    self.generate_expr((*obj).clone());
                    self.emitter.push(AsmRegister::X0);
                } else {
                    self.generate_expr((*obj).clone());
                    self.emitter.push(AsmRegister::X0);
                }

                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(AsmRegister::X0);
                }

                let num_args = args.len() + 1;
                for i in (0..num_args.min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }

                let mut method_label = format!("_METHOD_{}", member);

                if let Some(cname) = class_name_found {
                    method_label = format!("_{}_{}", cname, member);
                } else if let Some(Type::Class(ref class_name)) = self.get_node_type(&obj_span) {
                    if !self.interfaces.contains(class_name)
                        && !self.abstract_classes.contains(class_name)
                    {
                        method_label = format!("_{}_{}", class_name, member);
                    }
                }

                if method_label.starts_with("_METHOD_") {
                    if is_primitive {
                        let mut ty = self
                            .get_node_type(&obj_span)
                            .cloned()
                            .unwrap_or(Type::Unknown);
                        if matches!(ty, Type::Unknown) {
                            match &*obj {
                                Expr::StringLiteral(_, _) => ty = Type::String,
                                Expr::ArrayLiteral(_, _) => {
                                    ty = Type::Array(Box::new(Type::Unknown))
                                }
                                Expr::Variable(ref name, _) => {
                                    if let Some((_, var_ty)) = self.variables.get(name) {
                                        ty = var_ty.clone();
                                    } else if let Some((_, var_ty)) =
                                        self.global_variables.get(name)
                                    {
                                        ty = var_ty.clone();
                                    }
                                }
                                _ => {}
                            }
                        }
                        if matches!(ty, Type::Unknown) {
                            if matches!(
                                member.as_str(),
                                "charAt"
                                    | "substring"
                                    | "indexOf"
                                    | "toUpper"
                                    | "toLower"
                                    | "trim"
                                    | "len"
                            ) {
                                ty = Type::String;
                            } else if matches!(
                                member.as_str(),
                                "push" | "pop" | "join" | "get" | "len"
                            ) {
                                ty = Type::Array(Box::new(Type::Unknown));
                            }
                        }
                        if matches!(ty, Type::String) {
                            method_label = format!("_aura_string_{}", member);
                        } else {
                            method_label = format!("_aura_array_{}", member);
                        }
                    } else {
                        // Fallback search
                        for (class_name, (_, methods)) in &self.classes {
                            if methods.contains(&member) {
                                method_label = format!("_{}_{}", class_name, member);
                                break;
                            }
                        }

                        if method_label.starts_with("_METHOD_") {
                            if matches!(
                                member.as_str(),
                                "charAt" | "substring" | "indexOf" | "toUpper" | "toLower" | "trim"
                            ) {
                                method_label = format!("_aura_string_{}", member);
                            } else if matches!(member.as_str(), "push" | "pop" | "join" | "get") {
                                method_label = format!("_aura_array_{}", member);
                            } else if member == "len" {
                                method_label = format!("_aura_string_{}", member);
                            }
                        }
                    }
                }

                if is_static || is_primitive {
                    self.emitter.call(&method_label);
                } else if let Some(idx) = self.method_to_idx.get(&member) {
                    // Virtual call: x0 contains 'this'
                    self.emitter.output.push_str("    ldr x16, [x0]\n"); // Load VTable pointer
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x16, [x16, #{}]\n", idx * 8)); // Load method
                    self.emitter.output.push_str("    blr x16\n");
                } else {
                    // Fallback to direct call if not in vtable (e.g. private or non-virtual?)
                    self.emitter.call(&method_label);
                }
            }
            Expr::Call(name, _, args, _) => {
                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(AsmRegister::X0);
                }
                for i in (0..args.len().min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }
                if self.variables.contains_key(&name) {
                    if let Some((offset, _)) = self.variables.get(&name) {
                        self.load_local("x16", *offset);
                        self.emitter.output.push_str("    blr x16\n");
                    }
                } else {
                    let call_label = if name == "main" {
                        "_main_aura".to_string()
                    } else {
                        format!("_{}", name)
                    };
                    self.emitter.call(&call_label);
                }
            }
            Expr::UnaryOp(op, expr, _) => {
                self.generate_expr(*expr);
                if op == "-" {
                    self.emitter
                        .sub(AsmRegister::X0, AsmRegister::XZR, AsmRegister::X0);
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                let check_type_name =
                    if let crate::compiler::ast::TypeExpr::Name(ref name, _) = ty_expr {
                        name.as_str()
                    } else {
                        ""
                    };

                self.generate_expr(*expr);

                if check_type_name == "i64"
                    || check_type_name == "i32"
                    || check_type_name == "number"
                {
                    // Check if x0 != 0 AND not in string pointer range.
                    self.emitter
                        .output
                        .push_str("    cmp x0, #0\n    cset x1, ne\n");
                    self.emitter.mov_imm(AsmRegister::X2, 0x100000000); // 4GB
                    self.emitter
                        .output
                        .push_str("    cmp x0, x2\n    cset x2, ge\n");
                    self.emitter.mov_imm(AsmRegister::X3, 0x200000000); // 8GB
                    self.emitter
                        .output
                        .push_str("    cmp x0, x3\n    cset x3, lt\n");
                    self.emitter.output.push_str("    and x2, x2, x3\n"); // 1 if in range (string)
                    self.emitter.output.push_str("    eor x2, x2, #1\n"); // 1 if NOT in range
                    self.emitter.output.push_str("    and x0, x1, x2\n"); // x0 = not null AND not string
                } else if check_type_name == "string" {
                    self.emitter.mov_imm(AsmRegister::X2, 0x100000000);
                    self.emitter
                        .output
                        .push_str("    cmp x0, x2\n    cset x2, ge\n");
                    self.emitter.mov_imm(AsmRegister::X3, 0x200000000);
                    self.emitter
                        .output
                        .push_str("    cmp x0, x3\n    cset x3, lt\n");
                    self.emitter.output.push_str("    and x0, x2, x3\n");
                } else {
                    // Fail fallback
                    self.emitter.mov_imm(AsmRegister::X0, 0);
                }
            }
            Expr::Template(parts, _) => {
                for (i, part) in parts.into_iter().enumerate() {
                    match part {
                        TemplatePart::Str(s) => {
                            let label = if let Some(l) = self.string_constants.get(&s) {
                                l.clone()
                            } else {
                                let l = self.new_label("str");
                                self.string_constants.insert(s.clone(), l.clone());
                                l
                            };
                            self.emitter
                                .output
                                .push_str(&format!("    adrp x0, {}@PAGE\n", label));
                            self.emitter
                                .output
                                .push_str(&format!("    add x0, x0, {}@PAGEOFF\n", label));
                        }
                        TemplatePart::Expr(expr) => {
                            let span = expr.span();
                            let mut ty =
                                self.get_node_type(&span).cloned().unwrap_or(Type::Unknown);
                            if matches!(ty, Type::Unknown | Type::Int64) {
                                if let Expr::Variable(ref name, _) = *expr {
                                    if name == "true" || name == "false" {
                                        ty = Type::Boolean;
                                    } else if let Some((_, var_ty)) = self.variables.get(name) {
                                        ty = var_ty.clone();
                                    }
                                }
                            }
                            self.generate_expr((*expr).clone());
                            match ty {
                                Type::Int32 | Type::Int64 | Type::Unknown => {
                                    self.emitter.call("_aura_num_to_str");
                                }
                                Type::Boolean => {
                                    self.emitter.call("_aura_bool_to_str");
                                }
                                Type::String => {}
                                _ => {
                                    self.emitter.call("_aura_num_to_str");
                                }
                            }
                        }
                    }
                    if i > 0 {
                        self.emitter.pop(AsmRegister::X1); // Previous result
                        self.emitter.mov_reg(AsmRegister::X2, AsmRegister::X0); // current
                        self.emitter.mov_reg(AsmRegister::X0, AsmRegister::X1); // previous
                        self.emitter.mov_reg(AsmRegister::X1, AsmRegister::X2); // current
                        self.emitter.call("_aura_str_concat");
                    }
                    self.emitter.push(AsmRegister::X0);
                }
                self.emitter.pop(AsmRegister::X0);
            }
            Expr::ArrayLiteral(elements, _) => {
                self.emitter.mov_imm(AsmRegister::X0, elements.len() as i64);
                self.emitter.call("_aura_array_new");
                self.emitter.push(AsmRegister::X0);
                for el in elements {
                    self.generate_expr(el);
                    self.emitter.mov_reg(AsmRegister::X1, AsmRegister::X0);
                    self.emitter.pop(AsmRegister::X0);
                    self.emitter.push(AsmRegister::X0);
                    self.emitter.call("_aura_array_push");
                }
                self.emitter.pop(AsmRegister::X0);
            }
            Expr::Await(expr, _) => {
                self.generate_expr(*expr);
            }
            Expr::Throw(expr, _) => {
                self.generate_expr(*expr);
                self.emitter.call("_aura_throw");
            }
            Expr::Index(obj, index, _) => {
                self.generate_expr(*obj);
                self.emitter.push(AsmRegister::X0);
                self.generate_expr(*index);
                self.emitter.mov_reg(AsmRegister::X1, AsmRegister::X0);
                self.emitter.pop(AsmRegister::X0);
                self.emitter.call("_aura_array_get");
            }
            Expr::Super(_) => {
                let (offset, _) = self
                    .variables
                    .get("this")
                    .expect("'super' used outside of method");
                self.load_local("x0", *offset);
            }
            Expr::SuperCall(_, _) => {
                // Not fully supported in AST-based codegen, but needs to be exhaustive
            }
            Expr::Error(_) => panic!("Compiler bug: reaching error node in codegen"),
        }
    }
}
