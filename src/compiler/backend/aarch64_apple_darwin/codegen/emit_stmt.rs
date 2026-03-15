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
                    .unwrap_or(Type::Unknown);
                if matches!(var_ty, Type::Unknown | Type::Int64) {
                    match value {
                        Expr::StringLiteral(_, _) => var_ty = Type::String,
                        Expr::Variable(ref n, _) if n == "true" || n == "false" => {
                            var_ty = Type::Boolean
                        }
                        Expr::ArrayLiteral(_, _) => var_ty = Type::Array(Box::new(Type::Unknown)),
                        Expr::MethodCall(_, ref member, _, _, _) => {
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
                if is_method {
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
                        .unwrap_or(Type::Unknown);
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
                if let Some(ty) = self.get_node_type(&expr.span()) {
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

                // Check if this is a string-backed enum variable
                if !is_str {
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

                if !is_str && !is_bool && !is_array && !is_promise && !is_null {
                    match &expr {
                        Expr::StringLiteral(_, _) => is_str = true,
                        Expr::BinaryOp(ref left, ref op, ref right, _) if op == "+" => {
                            if matches!(&**left, Expr::StringLiteral(_, _))
                                || matches!(&**right, Expr::StringLiteral(_, _))
                            {
                                is_str = true;
                            }
                        }
                        Expr::MethodCall(_, ref member, _, _, _) => {
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

                self.generate_expr(expr);
                if is_str {
                    self.emitter.call("_print_str");
                } else if is_bool {
                    self.emitter.call("_print_bool");
                } else if is_array {
                    self.emitter.call("_print_array");
                } else if is_promise {
                    self.emitter.call("_print_promise");
                } else if is_null {
                    // No print_null yet
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
            Statement::ClassDeclaration {
                name,
                name_span: _,
                fields,
                methods,
                constructor,
                extends,
                implements: _,
                is_abstract: _,
                span,
                doc: _,
            } => {
                let mut vtable_methods = Vec::new();
                if let Some(ref parent_name) = extends {
                    if let Some(parent_vtable) = self.vtables.get(parent_name).cloned() {
                        vtable_methods = parent_vtable;
                    }
                }

                for m in &methods {
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
                            vtable_methods[idx as usize] = format!("{}_{}", name, m.name);
                        }
                    }
                }
                self.vtables.insert(name.clone(), vtable_methods.clone());

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
                    .insert(name.clone(), (field_names, method_names));

                let old_class = self.current_class.replace(name.clone());
                let saved_global_scope = self.is_global_scope;
                self.is_global_scope = false;

                if let Some(cons) = constructor {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_ctor", name),
                        name_span: cons.name_span,
                        params: cons.params,
                        return_ty: cons.return_ty,
                        body: cons.body,
                        is_async: cons.is_async,
                        span: cons.span,
                        doc: None,
                    });
                } else {
                    // Default constructor
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_ctor", name),
                        name_span: span,
                        params: vec![],
                        return_ty: TypeExpr::Name("void".to_string(), span),
                        body: Box::new(Statement::Block(vec![], span)),
                        is_async: false,
                        span,
                        doc: None,
                    });
                }

                for method in &methods {
                    if method.is_abstract {
                        continue;
                    }
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_{}", name, method.name),
                        name_span: method.name_span,
                        params: method.params.clone(),
                        return_ty: method.return_ty.clone(),
                        body: method.body.clone(),
                        is_async: method.is_async,
                        span: method.span,
                        doc: None,
                    });
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
