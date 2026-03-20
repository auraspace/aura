use crate::compiler::ast::{Expr, Span, Statement};
use crate::compiler::frontend::error::Diagnostic;
use crate::compiler::sema::checker::SemanticAnalyzer;
use crate::compiler::sema::checker::SemanticErrorKind;
use crate::compiler::sema::ty::Type;

impl SemanticAnalyzer {
    pub fn check_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::Enum(decl) => {
                let mut first_ty: Option<Type> = None;

                for member in &decl.members {
                    let member_ty = if let Some(ref expr) = member.value {
                        let ty = self.check_expr(expr.clone());
                        if !matches!(ty, Type::Int64 | Type::Int32 | Type::String) {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    "Int or String".to_string(),
                                    format!("{:?}", ty),
                                ),
                                member.name_span,
                            );
                        }

                        ty
                    } else {
                        // Implicit value
                        if let Some(Type::String) = first_ty {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    "Explicit String value required for all members".to_string(),
                                    "Implicit Enum value".to_string(),
                                ),
                                member.name_span,
                            );
                        }
                        // It's implicitly an integer
                        Type::Int64
                    };

                    if let Some(ref first) = first_ty {
                        // Check if all members have the same primitive type base
                        let base_first = if matches!(first, Type::Int64 | Type::Int32) {
                            Type::Int64
                        } else {
                            first.clone()
                        };
                        let base_current = if matches!(member_ty, Type::Int64 | Type::Int32) {
                            Type::Int64
                        } else {
                            member_ty.clone()
                        };

                        if base_first != base_current && base_current != Type::Error {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    format!("{:?}", first),
                                    format!("{:?}", member_ty),
                                ),
                                member.name_span,
                            );
                        }
                    } else {
                        first_ty = Some(member_ty.clone());
                    }

                    // Register enum member as a constant
                    // E.g., `Direction.Up` will be registered as `Direction.Up` in the scope
                    let fqn = format!("{}.{}", decl.name, member.name);
                    let is_exported = self
                        .scope
                        .lookup_local(&decl.name)
                        .map(|s| s.is_exported)
                        .unwrap_or(false);
                    self.scope.insert(
                        fqn,
                        Type::Enum(decl.name.clone()),
                        false,
                        true, // Enum members are constant
                        is_exported,
                        member.name_span,
                        self.current_file.clone(),
                        None,
                    );
                }

                // Register the Enum type itself
                let is_exported = self
                    .scope
                    .lookup_local(&decl.name)
                    .map(|s| s.is_exported)
                    .unwrap_or(false);
                self.scope.insert(
                    decl.name.clone(),
                    Type::Enum(decl.name.clone()),
                    false,
                    true, // Enum itself is a constant symbol
                    is_exported,
                    decl.name_span,
                    self.current_file.clone(),
                    decl.doc.as_ref().map(|d| d.content()),
                );
            }
            Statement::VarDeclaration {
                name,
                name_span,
                ty,
                value,
                is_const,
                span,
                doc,
            } => {
                if self.scope.parent.is_some() {
                    if self.scope.lookup_local(&name).is_some() {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(name.clone()),
                            name_span,
                        );
                    }
                }
                let val_span = value.span();
                let val_ty = self.check_expr(value);
                let declared_ty = if let Some(t) = ty {
                    self.resolve_type(t)
                } else {
                    val_ty.clone()
                };
                if !self.is_assignable(&val_ty, &declared_ty) {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            format!("{:?}", declared_ty),
                            format!("{:?}", val_ty),
                        ),
                        span,
                    );
                } else if val_ty != declared_ty && self.record_node_info {
                    self.record_type(val_span, declared_ty.clone());
                }
                if self.record_node_info {
                    if let Some(d) = &doc {
                        self.record_doc(name_span, d.content());
                    }
                    self.record_type(name_span, declared_ty.clone());
                }
                let is_exported_flag = self
                    .scope
                    .lookup_local(&name)
                    .map(|s| s.is_exported)
                    .unwrap_or(false);
                self.scope.insert(
                    name,
                    declared_ty,
                    false,
                    is_const,
                    is_exported_flag,
                    span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
            }
            Statement::Expression(expr, _) => {
                self.check_expr(expr);
            }
            Statement::Print(expr, _) => {
                self.check_expr(expr);
            }
            Statement::Block(stmts, _) => {
                self.push_scope();
                for s in stmts {
                    self.check_statement(s);
                }
                self.pop_scope();
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let _cond_ty = self.check_expr(condition.clone());

                if let Expr::TypeTest(ref expr, ref ty_expr, _) = condition {
                    if let Expr::Variable(ref name, _) = **expr {
                        let narrowed_ty = self.resolve_type(ty_expr.clone());

                        self.push_scope();
                        self.scope.insert(
                            name.clone(),
                            narrowed_ty.clone(),
                            false,
                            false,
                            false, // narrowed type in block
                            span,
                            self.current_file.clone(),
                            None,
                        );
                        self.check_statement(*then_branch);
                        self.pop_scope();

                        if let Some(eb) = else_branch {
                            let original_ty = self
                                .scope
                                .lookup(name)
                                .map(|s| s.ty.clone())
                                .unwrap_or(Type::Error);
                            let excluded_ty = original_ty.exclude(&narrowed_ty);

                            self.push_scope();
                            self.scope.insert(
                                name.clone(),
                                excluded_ty,
                                false,
                                false,
                                false, // narrowed type in block
                                span,
                                self.current_file.clone(),
                                None,
                            );
                            self.check_statement(*eb);
                            self.pop_scope();
                        }
                        return;
                    }
                }

                self.check_statement(*then_branch);
                if let Some(eb) = else_branch {
                    self.check_statement(*eb);
                }
            }
            Statement::While {
                condition,
                body,
                span,
            } => {
                let cond_ty = self.check_expr(condition);
                if cond_ty != Type::Boolean && cond_ty != Type::Error {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "boolean".to_string(),
                            format!("{}", cond_ty),
                        ),
                        span,
                    );
                }
                self.check_statement(*body);
            }
            Statement::For {
                initializer,
                condition,
                increment,
                body,
                span,
            } => {
                self.push_scope();
                if let Some(init) = initializer {
                    self.check_statement(*init);
                }
                if let Some(cond) = condition {
                    let cond_ty = self.check_expr(cond);
                    if cond_ty != Type::Boolean && cond_ty != Type::Error {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                "boolean".to_string(),
                                format!("{}", cond_ty),
                            ),
                            span,
                        );
                    }
                }
                if let Some(inc) = increment {
                    self.check_expr(inc);
                }
                self.check_statement(*body);
                self.pop_scope();
            }
            Statement::ForOf {
                variable,
                variable_span,
                is_const,
                iterable,
                body,
                span,
            } => {
                let iterable_ty = self.check_expr(iterable);
                let element_ty = match iterable_ty {
                    Type::Array(inner) => *inner,
                    Type::Error => Type::Error,
                    _ => {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                "Array".to_string(),
                                format!("{}", iterable_ty),
                            ),
                            span,
                        );
                        Type::Error
                    }
                };

                self.push_scope();
                self.scope.insert(
                    variable,
                    element_ty,
                    false,
                    is_const,
                    false, // Loop variable is not exported
                    variable_span,
                    self.current_file.clone(),
                    None,
                );
                self.check_statement(*body);
                self.pop_scope();
            }
            Statement::Return(expr, _) => {
                self.check_expr(expr);
            }
            Statement::FunctionDeclaration {
                name,
                name_span,
                type_params,
                params,
                return_ty,
                body,
                is_async: _,
                span,
                doc,
            } => {
                if self.scope.parent.is_some() {
                    if self.scope.lookup_local(&name).is_some() {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(name.clone()),
                            name_span,
                        );
                    }
                }
                let mut param_tys = Vec::new();
                for (_, ty) in &params {
                    param_tys.push(self.resolve_type(ty.clone()));
                }
                let ret_ty = self.resolve_type(return_ty);

                // Register function before checking body for recursion
                let func_ty = Type::Function(
                    type_params.clone(),
                    param_tys.clone(),
                    Box::new(ret_ty.clone()),
                );
                let is_exported_flag = self
                    .scope
                    .lookup_local(&name)
                    .map(|s| s.is_exported)
                    .unwrap_or(false);
                self.scope.insert(
                    name.clone(),
                    func_ty.clone(),
                    false,
                    true, // functions are constant
                    is_exported_flag,
                    span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
                if self.record_node_info {
                    if let Some(d) = &doc {
                        self.record_doc(name_span, d.content());
                    }
                    self.record_type(name_span, func_ty.clone());
                }

                self.push_scope();
                self.current_method = Some(name.clone());

                // Push type parameters into scope
                for tp in &type_params {
                    self.scope.insert(
                        tp.name.clone(),
                        Type::GenericParam(tp.name.clone()),
                        false,
                        true,
                        false,
                        tp.span,
                        self.current_file.clone(),
                        None,
                    );
                }

                for (pname, pty) in params {
                    let ty = self.resolve_type(pty.clone());
                    self.scope.insert(
                        pname,
                        ty,
                        true,
                        false,
                        false,
                        pty.span(),
                        self.current_file.clone(),
                        None,
                    );
                }
                self.check_statement(*body);
                self.current_method = None;
                self.pop_scope();
            }
            Statement::ClassDeclaration {
                name,
                name_span: _,
                fields,
                methods,
                constructor,
                extends: _,
                implements: _,
                is_abstract: _,
                span: _,
                doc: _,
                type_params,
            } => {
                self.current_class = Some(name.clone());
                self.push_scope();

                // Push type parameters into scope
                for tp in &type_params {
                    self.scope.insert(
                        tp.name.clone(),
                        Type::GenericParam(tp.name.clone()),
                        false,
                        true,
                        false,
                        tp.span,
                        self.current_file.clone(),
                        None,
                    );
                }

                for f in fields {
                    if let Some(init) = &f.value {
                        let init_ty = self.check_expr(init.clone());
                        let expected_ty = self.resolve_type(f.ty.clone());
                        if init_ty != expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                format!(
                                    "Type mismatch in field initializer for {}: expected {}, found {}",
                                    f.name, expected_ty, init_ty
                                ),
                                f.span.line,
                                f.span.column,
                            ));
                        }
                    }
                }

                if let Some(ctor) = constructor {
                    self.push_scope();
                    self.current_method = Some("constructor".to_string());
                    self.scope.insert(
                        "this".to_string(),
                        Type::Class(name.clone()),
                        false,
                        true,  // this is constant
                        false, // this is not exported
                        ctor.span,
                        self.current_file.clone(),
                        None,
                    );
                    for (pname, pty) in ctor.params {
                        let ty = self.resolve_type(pty.clone());
                        self.scope.insert(
                            pname,
                            ty,
                            true,
                            false,
                            false, // param not exported
                            pty.span(),
                            self.current_file.clone(),
                            None,
                        );
                    }
                    self.check_statement(*ctor.body);
                    self.current_method = None;
                    self.pop_scope();
                }

                for m in methods {
                    self.push_scope();
                    self.current_method = Some(m.name.clone());
                    self.is_static_context = m.is_static;
                    if !m.is_static {
                        self.scope.insert(
                            "this".to_string(),
                            Type::Class(name.clone()),
                            false,
                            true,  // this is constant
                            false, // this is not exported
                            m.span,
                            self.current_file.clone(),
                            None,
                        );
                    }
                    for (pname, pty) in m.params {
                        let ty = self.resolve_type(pty.clone());
                        self.scope.insert(
                            pname,
                            ty,
                            true,
                            false,
                            false, // param not exported
                            pty.span(),
                            self.current_file.clone(),
                            None,
                        );
                    }
                    self.check_statement(*m.body);
                    self.current_method = None;
                    self.is_static_context = false;
                    self.pop_scope();
                }
                self.pop_scope();
                self.current_class = None;
            }
            Statement::Error => {}
            Statement::Import { .. } => {
                // Symbols should be loaded in Pass 1 (collect_definitions)
                // But we should also check the body of the imported file for errors
                // However, we need to be careful not to check it multiple times.
                // For now, Pass 1 loading is enough for symbols.
                // To support full error checking of dependencies, we'd need a more robust module system.
            }
            Statement::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
                ..
            } => {
                self.check_statement(*try_block);

                if let Some(cb) = catch_block {
                    self.push_scope();

                    if let Some((name, ty_expr)) = catch_param {
                        let ty = self.resolve_type(ty_expr);
                        // If type is Unknown (not provided), we might want to default to Error
                        let final_ty = if matches!(ty, Type::Error) {
                            Type::Class("Error".to_string())
                        } else {
                            ty
                        };
                        self.scope.insert(
                            name.clone(),
                            final_ty,
                            true,
                            false,
                            false, // catch param not exported
                            Span::new(0, 0),
                            self.current_file.clone(),
                            None,
                        );
                    }

                    self.check_statement(*cb);
                    self.pop_scope();
                }

                if let Some(fb) = finally_block {
                    self.check_statement(*fb);
                }
            }
            Statement::Export { decl, .. } => {
                self.check_statement(*decl);
            }
            Statement::Interface(_) | Statement::TypeAlias(_) => {}
            Statement::Comment(_, _)
            | Statement::RegularBlockComment(_, _)
            | Statement::Empty(_) => {}
        }
    }
}
