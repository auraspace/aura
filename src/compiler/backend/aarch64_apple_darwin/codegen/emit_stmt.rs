use super::Codegen;
use crate::compiler::ast::{Expr, Statement, TypeExpr};
use crate::compiler::sema::ty::Type;

impl Codegen {
    pub fn generate_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::Enum(_) => {}
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _) => {}
            Statement::VarDeclaration {
                name, ty: _, value, ..
            } => {
                let mut var_ty = self
                    .get_node_type(&value.span())
                    .cloned()
                    .unwrap_or(Type::Error);
                if matches!(var_ty, Type::Error | Type::Int64) {
                    match value {
                        Expr::StringLiteral(_, _) => var_ty = Type::String,
                        Expr::Variable(ref n, _) if n == "true" || n == "false" => {
                            var_ty = Type::Boolean
                        }
                        Expr::ArrayLiteral(_, _) => var_ty = Type::Array(Box::new(Type::Error)),
                        Expr::MethodCall(_, ref member, _, _, _, _) => {
                            if matches!(
                                member.as_str(),
                                "trim"
                                    | "len"
                                    | "toUpper"
                                    | "toLower"
                                    | "charAt"
                                    | "substring"
                                    | "join"
                                    | "read"
                            ) {
                                var_ty = Type::String;
                            }
                        }
                        _ => {}
                    }
                }
                self.generate_expr(value);
                if self.is_global_scope {
                    let (label, _) = self
                        .global_variables
                        .get(&name)
                        .expect("Global variable should be predefined");
                    self.emitter
                        .output
                        .push_str(&format!("    adrp x1, {}@PAGE\n", label));
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x1, {}@PAGEOFF]\n", label));
                } else {
                    self.stack_offset += 16;
                    self.variables
                        .insert(name.clone(), (self.stack_offset, var_ty));
                    self.store_local("x0", self.stack_offset);
                }
            }
            Statement::FunctionDeclaration {
                name,
                name_span: _,
                type_params: _,
                params,
                return_ty: _,
                body,
                is_async: _,
                span: _,
                doc: _,
            } => {
                let saved_vars = self.variables.clone();
                let saved_offset = self.stack_offset;
                let saved_global_scope = self.is_global_scope;
                self.variables.clear();
                self.stack_offset = 0;
                self.is_global_scope = false;

                let is_method = self.current_class.is_some() && !name.contains("main");

                let fn_label = if name == "main" {
                    "_main_aura".to_string()
                } else {
                    format!("_{}", name)
                };
                let end_label = self.new_label("fn_end");
                let old_fn_end = self.current_fn_end.replace(end_label.clone());

                self.emitter
                    .output
                    .push_str(&format!(".global {}\n{}:\n", fn_label, fn_label));
                self.emitter
                    .output
                    .push_str("    stp x29, x30, [sp, -16]!\n");
                self.emitter.output.push_str("    mov x29, sp\n");
                self.emitter.output.push_str("    sub sp, sp, #256\n");

                let mut current_arg_reg = 0;
                if is_method && !self.is_static_context {
                    self.stack_offset += 16;
                    let class_ty = Type::Class(self.current_class.clone().unwrap());
                    self.variables
                        .insert("this".to_string(), (self.stack_offset, class_ty));
                    self.store_local("x0", self.stack_offset);
                    current_arg_reg += 1;
                }

                // Map params to stack
                for (pname, ty_expr) in params {
                    let pty = self
                        .get_node_type(&ty_expr.span())
                        .cloned()
                        .unwrap_or(Type::Error);
                    self.stack_offset += 16;
                    self.variables
                        .insert(pname.clone(), (self.stack_offset, pty));
                    if current_arg_reg < 8 {
                        self.store_local(&format!("x{}", current_arg_reg), self.stack_offset);
                        current_arg_reg += 1;
                    }
                }

                self.generate_statement(*body);

                self.emitter.output.push_str(&format!("{}:\n", end_label));
                self.emitter.output.push_str("    add sp, sp, #256\n");
                self.emitter.output.push_str("    ldp x29, x30, [sp], 16\n");
                self.emitter.output.push_str("    ret\n");

                self.variables = saved_vars;
                self.stack_offset = saved_offset;
                self.is_global_scope = saved_global_scope;
                self.current_fn_end = old_fn_end;
            }
            Statement::Return(expr, _) => {
                self.generate_expr(expr);
                if let Some(ref end) = self.current_fn_end {
                    self.emitter.output.push_str(&format!("    b {}\n", end));
                }
            }
            Statement::Print(expr, _) => {
                let mut is_str = matches!(expr, Expr::StringLiteral(_, _) | Expr::Template(_, _));
                let mut is_bool = false;
                let mut is_array = false;
                let mut is_promise = false;
                let mut is_null = matches!(expr, Expr::Null(_));

                if let Expr::Variable(ref name, _) = expr {
                    if let Some((_, ty)) = self.variables.get(name) {
                        match ty {
                            Type::String => is_str = true,
                            Type::Boolean => is_bool = true,
                            Type::Array(_) => is_array = true,
                            Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                            Type::Null => is_null = true,
                            Type::Enum(ref enum_name) => {
                                if self.is_string_enum(enum_name) {
                                    is_str = true;
                                }
                            }
                            _ => {}
                        }
                    } else if let Some((_, ty)) = self.global_variables.get(name) {
                        match ty {
                            Type::String => is_str = true,
                            Type::Boolean => is_bool = true,
                            Type::Array(_) => is_array = true,
                            Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                            Type::Null => is_null = true,
                            Type::Enum(ref enum_name) => {
                                if self.is_string_enum(enum_name) {
                                    is_str = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let node_ty = self.get_node_type(&expr.span());
                let mut specialized_class_name = None;
                let mut has_to_string = false;
                let mut class_name_for_default = None;

                if let Some(ty) = node_ty {
                    let actual_ty = self.get_specialized_type(&ty);
                    match actual_ty {
                        Type::String => is_str = true,
                        Type::Boolean => is_bool = true,
                        Type::Array(_) => is_array = true,
                        Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                        Type::Null => is_null = true,
                        Type::Enum(ref enum_name) => {
                            if self.is_string_enum(enum_name) {
                                is_str = true;
                            }
                        }
                        Type::Class(ref name) => {
                            class_name_for_default = Some(name.clone());
                            if self.has_method(name, "toString") {
                                has_to_string = true;
                                specialized_class_name = Some(name.clone());
                            }
                        }
                        Type::Generic(ref name, ref args) => {
                            let mangled = self.mangle_name(name, args);
                            class_name_for_default = Some(mangled.clone());
                            if self.has_method(&mangled, "toString") {
                                has_to_string = true;
                                specialized_class_name = Some(mangled);
                            } else if self.has_method(name, "toString") {
                                has_to_string = true;
                                specialized_class_name = Some(name.clone());
                            }
                        }
                        _ => {}
                    }
                }

                // Check if this is a string-backed enum variable
                if !is_str && !has_to_string {
                    let enum_name: Option<String> = if let Expr::Variable(ref var_name, _) = expr {
                        let local_ty = self.variables.get(var_name).map(|(_, t)| t.clone());
                        let ty = local_ty.or_else(|| {
                            self.global_variables.get(var_name).map(|(_, t)| t.clone())
                        });
                        if let Some(Type::Enum(name)) = ty {
                            Some(name)
                        } else {
                            None
                        }
                    } else {
                        self.get_node_type(&expr.span()).and_then(|t| {
                            if let Type::Enum(ref n) = t {
                                Some(n.clone())
                            } else {
                                None
                            }
                        })
                    };
                    if let Some(ref name) = enum_name {
                        if self.is_string_enum(name) {
                            is_str = true;
                        }
                    }
                }

                if !is_str && !is_bool && !is_array && !is_promise && !is_null && !has_to_string {
                    match &expr {
                        Expr::StringLiteral(_, _) => is_str = true,
                        Expr::BinaryOp(ref left, ref op, ref right, _) if op == "+" => {
                            if matches!(&**left, Expr::StringLiteral(_, _))
                                || matches!(&**right, Expr::StringLiteral(_, _))
                            {
                                is_str = true;
                            }
                        }
                        Expr::MethodCall(_, ref member, _, _, _, _) => {
                            if matches!(
                                member.as_str(),
                                "charAt"
                                    | "substring"
                                    | "toUpper"
                                    | "toLower"
                                    | "trim"
                                    | "toString"
                                    | "join"
                                    | "read"
                            ) {
                                is_str = true;
                            }
                        }
                        _ => {}
                    }
                }

                let is_float = self
                    .get_node_type(&expr.span())
                    .map_or(false, |t| t.is_float());
                self.generate_expr(expr);
                if is_str {
                    self.emitter.call("_print_str");
                } else if has_to_string {
                    if let Some(class_name) = specialized_class_name {
                        self.emitter
                            .call(&format!("_{}_{}", class_name, "toString"));
                        self.emitter.call("_print_str");
                    }
                } else if is_bool {
                    self.emitter.call("_print_bool");
                } else if is_array {
                    self.emitter.call("_print_array");
                } else if is_promise {
                    self.emitter.call("_print_promise");
                } else if is_null {
                    // No print_null yet
                } else if is_float {
                    self.emitter.output.push_str("    fmov d0, x0\n");
                    self.emitter.call("_print_float");
                } else if let Some(class_name) = class_name_for_default {
                    let label = if let Some(l) = self.string_constants.get(&class_name) {
                        l.clone()
                    } else {
                        let l = format!("_s{}", self.string_constants.len());
                        self.string_constants.insert(class_name.clone(), l.clone());
                        l
                    };
                    self.emitter
                        .output
                        .push_str(&format!("    adrp x0, {}@PAGE\n", label));
                    self.emitter
                        .output
                        .push_str(&format!("    add x0, x0, {}@PAGEOFF\n", label));
                    self.emitter.call("_print_object_default");
                } else {
                    self.emitter.call("_print_num");
                }
            }
            Statement::Expression(expr, _) => {
                self.generate_expr(expr);
            }
            Statement::Block(stmts, _) => {
                for s in stmts {
                    self.generate_statement(s);
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span: _,
            } => {
                let else_label = self.new_label("else");
                let end_label = self.new_label("end");
                self.generate_expr(condition);
                self.emitter
                    .output
                    .push_str(&format!("    cbz x0, {}\n", else_label));
                self.generate_statement(*then_branch);
                self.emitter
                    .output
                    .push_str(&format!("    b {}\n", end_label));
                self.emitter.output.push_str(&format!("{}:\n", else_label));
                if let Some(eb) = else_branch {
                    self.generate_statement(*eb);
                }
                self.emitter.output.push_str(&format!("{}:\n", end_label));
            }
            Statement::While {
                condition,
                body,
                span: _,
            } => {
                let start_label = self.new_label("while_start");
                let end_label = self.new_label("while_end");
                self.emitter.output.push_str(&format!("{}:\n", start_label));
                self.generate_expr(condition);
                self.emitter
                    .output
                    .push_str(&format!("    cbz x0, {}\n", end_label));
                self.generate_statement(*body);
                self.emitter
                    .output
                    .push_str(&format!("    b {}\n", start_label));
                self.emitter.output.push_str(&format!("{}:\n", end_label));
            }
            Statement::For { .. } | Statement::ForOf { .. } => todo!("For loops in non-IR backend"),
            Statement::ClassDeclaration {
                ref name,
                name_span: _,
                type_params: _,
                ref fields,
                ref methods,
                ref constructor,
                ref extends,
                implements: _,
                is_abstract: _,
                span,
                doc: _,
            } => {
                let specialized_name =
                    if let Some((ref class_name, ref args)) = self.current_specialization {
                        if class_name == name {
                            self.mangle_name(class_name, args)
                        } else {
                            name.clone()
                        }
                    } else {
                        name.clone()
                    };

                let mut vtable_methods = Vec::new();
                if let Some(ext) = extends {
                    let parent_name = match ext {
                        TypeExpr::Name(n, _) => Some(n.clone()),
                        TypeExpr::Generic(n, _, _) => Some(n.clone()),
                        _ => None,
                    };
                    if let Some(pname) = parent_name {
                        if let Some(parent_vtable) = self.vtables.get(&pname).cloned() {
                            vtable_methods = parent_vtable;
                        }
                    }
                }

                for m in methods {
                    if !m.is_static {
                        let idx = self.method_to_idx.get(&m.name).cloned().unwrap_or_else(|| {
                            let new_idx = self.next_method_idx;
                            self.method_to_idx.insert(m.name.clone(), new_idx);
                            self.next_method_idx += 1;
                            new_idx
                        });

                        while vtable_methods.len() <= idx as usize {
                            vtable_methods.push("aura_null".to_string());
                        }
                        if m.is_abstract {
                            vtable_methods[idx as usize] = "aura_null".to_string();
                        } else {
                            vtable_methods[idx as usize] =
                                format!("{}_{}", specialized_name, m.name);
                        }
                    }
                }
                self.vtables
                    .insert(specialized_name.clone(), vtable_methods.clone());

                let field_names: Vec<String> = fields
                    .iter()
                    .filter(|f| !f.is_static)
                    .map(|f| f.name.clone())
                    .collect();
                let method_names: Vec<String> = methods
                    .iter()
                    .filter(|m| !m.is_static && !m.is_abstract)
                    .map(|m| m.name.clone())
                    .collect();
                self.classes
                    .insert(specialized_name.clone(), (field_names, method_names));

                let old_class = self.current_class.replace(specialized_name.clone());
                let saved_global_scope = self.is_global_scope;
                self.is_global_scope = false;

                if let Some(cons) = constructor {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_ctor", specialized_name),
                        name_span: cons.name_span,
                        type_params: cons.type_params.clone(),
                        params: cons.params.clone(),
                        return_ty: cons.return_ty.clone(),
                        body: cons.body.clone(),
                        is_async: cons.is_async,
                        span: cons.span,
                        doc: None,
                    });
                } else {
                    // Default constructor
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_ctor", specialized_name),
                        name_span: span,
                        type_params: vec![],
                        params: vec![],
                        return_ty: TypeExpr::Name("void".to_string(), span),
                        body: Box::new(Statement::Block(vec![], span)),
                        is_async: false,
                        span,
                        doc: None,
                    });
                }

                for method in methods {
                    if method.is_abstract {
                        continue;
                    }
                    self.is_static_context = method.is_static;
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_{}", specialized_name, method.name),
                        name_span: method.name_span,
                        type_params: method.type_params.clone(),
                        params: method.params.clone(),
                        return_ty: method.return_ty.clone(),
                        body: method.body.clone(),
                        is_async: method.is_async,
                        span: method.span,
                        doc: None,
                    });
                    self.is_static_context = false;
                }
                self.current_class = old_class;
                self.is_global_scope = saved_global_scope;
            }
            Statement::Interface(_) => {}
            Statement::Error => panic!("Compiler bug: reaching error node in codegen"),
            Statement::TryCatch { try_block, .. } => {
                // For now, just generate the try block to avoid panics
                self.generate_statement(*try_block);
            }
            Statement::Import { .. } => {
                // Handled in collect_all_definitions
            }
            Statement::Export { decl, .. } => {
                self.generate_statement(*decl);
            }
        }
    }
}
