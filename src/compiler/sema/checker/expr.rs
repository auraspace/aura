use crate::compiler::ast::Expr;
use crate::compiler::sema::checker::SemanticAnalyzer;
use crate::compiler::sema::checker::SemanticErrorKind;
use crate::compiler::sema::ty::Type;

impl SemanticAnalyzer {
    pub fn check_expr(&mut self, expr: Expr) -> Type {
        let span = expr.span();
        let ty = match expr {
            Expr::Number(_, _) => Type::Int32,
            Expr::StringLiteral(_, _) => Type::String,
            Expr::Template(parts, _s) => {
                for part in parts {
                    if let crate::compiler::ast::TemplatePart::Expr(e) = part {
                        self.check_expr(*e);
                    }
                }
                Type::String
            }
            Expr::Await(expr, _s) => {
                let ty = self.check_expr(*expr);
                // Basic validation: await target should be a Promise (or any for now)
                // For now, we just return the inner type if it's a Promise, or the type itself
                match ty {
                    Type::Generic(name, args) if name == "Promise" && args.len() == 1 => {
                        args[0].clone()
                    }
                    _ => ty, // Fallback
                }
            }
            Expr::ArrayLiteral(elements, _) => {
                let mut element_tys = Vec::new();
                for el in elements {
                    element_tys.push(self.check_expr(el));
                }
                let base_ty = if element_tys.is_empty() {
                    Type::Unknown
                } else {
                    let first = element_tys[0].clone();
                    if element_tys.iter().all(|t| *t == first) {
                        first
                    } else {
                        Type::Union(element_tys)
                    }
                };
                Type::Array(Box::new(base_ty))
            }
            Expr::Null(_) => Type::Null,
            Expr::Error(_s) => Type::Unknown,
            Expr::Variable(name, span) => {
                if let Some(sym) = self.scope.lookup(&name) {
                    let ty = sym.ty.clone();
                    if self.record_node_info {
                        let doc = sym.doc.clone();
                        let sym_span = sym.span;
                        let defined_in = sym.defined_in.clone();
                        if let Some(d) = doc {
                            self.record_doc(span, d);
                        }
                        self.record_definition(span, defined_in, sym_span);
                    }
                    ty
                } else if self.classes.contains_key(&name) {
                    Type::Class(name)
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                    Type::Unknown
                }
            }
            Expr::BinaryOp(left, op, right, span) => {
                let lhs = self.check_expr(*left);
                let rhs = self.check_expr(*right);
                match op.as_str() {
                    "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                        // Allow comparison between same types, or classes/unions/unknown and null
                        let is_nullable = |ty: &Type| {
                            matches!(
                                ty,
                                Type::Class(_) | Type::Union(_) | Type::Unknown | Type::Null
                            )
                        };
                        let ok = if lhs == rhs {
                            true
                        } else if is_nullable(&lhs) && is_nullable(&rhs) {
                            true
                        } else {
                            false
                        };

                        if !ok {
                            self.error(
                                SemanticErrorKind::IncompatibleBinaryOperators(
                                    lhs.to_string(),
                                    op,
                                    rhs.to_string(),
                                ),
                                span,
                            );
                        }
                        Type::Boolean
                    }
                    "&&" | "||" => {
                        if lhs.is_boolean() && rhs.is_boolean() {
                            Type::Boolean
                        } else {
                            self.error(
                                SemanticErrorKind::IncompatibleBinaryOperators(
                                    lhs.to_string(),
                                    op,
                                    rhs.to_string(),
                                ),
                                span,
                            );
                            Type::Boolean
                        }
                    }
                    "+" | "-" | "*" | "/" | "%" | "|" => {
                        if lhs.is_numeric() && rhs.is_numeric() {
                            lhs
                        } else if op == "+" && (lhs == Type::String || rhs == Type::String) {
                            Type::String
                        } else {
                            self.error(
                                SemanticErrorKind::IncompatibleBinaryOperators(
                                    lhs.to_string(),
                                    op,
                                    rhs.to_string(),
                                ),
                                span,
                            );
                            Type::Unknown
                        }
                    }
                    _ => Type::Unknown,
                }
            }
            Expr::Assign(name, value, span) => {
                let val_ty = self.check_expr(*value);
                let (sym_is_const, sym_ty) = if let Some(sym) = self.scope.lookup(&name) {
                    (sym.is_const, Some(sym.ty.clone()))
                } else {
                    (false, None)
                };

                if let Some(expected_ty) = sym_ty {
                    if sym_is_const {
                        self.error(
                            SemanticErrorKind::CannotAssignToConstant(name.clone()),
                            span,
                        );
                    }
                    if !self.is_assignable(&val_ty, &expected_ty) {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                format!("{:?}", expected_ty),
                                format!("{:?}", val_ty),
                            ),
                            span,
                        );
                    }
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                }
                val_ty
            }
            Expr::Call(name, name_span, args, span) => {
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                let sym = self.scope.lookup(&name);
                let function_ty = sym.map(|s| s.ty.clone());

                if let Some(Type::Function(param_tys, ret_ty)) = function_ty {
                    if self.record_node_info {
                        if let Some(sym) = self.scope.lookup(&name) {
                            let doc = sym.doc.clone();
                            let sym_span = sym.span;
                            let defined_in = sym.defined_in.clone();
                            if let Some(d) = doc {
                                self.record_doc(name_span, d);
                            }
                            self.record_definition(name_span, defined_in, sym_span);
                            self.record_type(
                                name_span,
                                Type::Function(param_tys.clone(), ret_ty.clone()),
                            );
                        }
                        // Also record return type for the whole call span
                        self.record_type(span, (*ret_ty).clone());
                    }
                    if param_tys.len() != arg_tys.len() {
                        self.error(
                            SemanticErrorKind::WrongArgumentCount(
                                name,
                                param_tys.len(),
                                arg_tys.len(),
                            ),
                            span,
                        );
                        return (*ret_ty).clone();
                    }
                    for (i, arg_ty) in arg_tys.iter().enumerate() {
                        if !self.is_assignable(arg_ty, &param_tys[i]) {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    format!("{:?}", param_tys[i]),
                                    format!("{:?}", arg_ty),
                                ),
                                span,
                            );
                        }
                    }
                    *ret_ty
                } else {
                    self.error(SemanticErrorKind::UndefinedFunction(name), name_span);
                    Type::Unknown
                }
            }
            Expr::New(class_name, name_span, args, span) => {
                if let Some(class_info) = self.classes.get(&class_name) {
                    if self.record_node_info {
                        let doc_opt = class_info.doc.clone();
                        let class_span = class_info.span;
                        if let Some(doc) = doc_opt {
                            self.record_doc(name_span, doc);
                        }
                        self.record_definition(name_span, self.current_file.clone(), class_span); // Class defined in same file for now if simple
                        self.record_type(name_span, Type::Class(class_name.clone()));
                        // Also record for the whole expression
                        self.record_type(span, Type::Class(class_name.clone()));
                    }
                } else {
                    self.error(SemanticErrorKind::UndefinedClass(class_name.clone()), span);
                }
                for arg in args {
                    self.check_expr(arg);
                }
                Type::Class(class_name)
            }
            Expr::MemberAccess(obj, field, name_span, span) => {
                let obj_ty = self.check_expr(*obj);

                if let Type::Class(ref class_name) | Type::Enum(ref class_name) = obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        let field_info = class_info
                            .fields
                            .get(&field)
                            .or(class_info.static_fields.get(&field))
                            .cloned();

                        if let Some((field_ty, field_span, doc)) = field_info {
                            if self.record_node_info {
                                if let Some(d) = doc {
                                    self.record_doc(name_span, d);
                                }
                                self.record_definition(
                                    name_span,
                                    self.current_file.clone(),
                                    field_span,
                                ); // Same file for simplicity now
                                self.record_type(name_span, field_ty.clone());
                                self.record_type(span, field_ty.clone());
                            }
                            field_ty
                        } else {
                            if matches!(obj_ty, Type::Enum(_)) {
                                // For enums, members are registered directly in the scope as Name.Member
                                let fqn = format!("{}.{}", class_name, field);
                                if let Some(sym) = self.scope.lookup(&fqn) {
                                    if self.record_node_info {
                                        let sym_ty = sym.ty.clone();
                                        let sym_doc = sym.doc.clone();
                                        let sym_defined_in = sym.defined_in.clone();
                                        let sym_span = sym.span;

                                        if let Some(d) = sym_doc {
                                            self.record_doc(name_span, d);
                                        }
                                        self.record_definition(name_span, sym_defined_in, sym_span);
                                        self.record_type(name_span, sym_ty.clone());
                                        self.record_type(span, sym_ty.clone());
                                        return sym_ty;
                                    }
                                    return sym.ty.clone();
                                }
                                self.error(
                                    SemanticErrorKind::UndefinedField(class_name.clone(), field),
                                    name_span,
                                );
                                return Type::Unknown;
                            }
                            self.error(
                                SemanticErrorKind::UndefinedField(class_name.clone(), field),
                                name_span,
                            );
                            Type::Unknown
                        }
                    } else {
                        if matches!(obj_ty, Type::Enum(_)) {
                            // Enum itself is just a namespace for its members
                            let fqn = format!("{}.{}", class_name, field);
                            if let Some(sym) = self.scope.lookup(&fqn) {
                                if self.record_node_info {
                                    let sym_ty = sym.ty.clone();
                                    let sym_doc = sym.doc.clone();
                                    let sym_defined_in = sym.defined_in.clone();
                                    let sym_span = sym.span;

                                    if let Some(d) = sym_doc {
                                        self.record_doc(name_span, d);
                                    }
                                    self.record_definition(name_span, sym_defined_in, sym_span);
                                    self.record_type(name_span, sym_ty.clone());
                                    self.record_type(span, sym_ty.clone());
                                    return sym_ty;
                                }
                                return sym.ty.clone();
                            }
                            self.error(
                                SemanticErrorKind::UndefinedField(class_name.clone(), field),
                                name_span,
                            );
                        } else {
                            self.error(SemanticErrorKind::UndefinedClass(class_name.clone()), span);
                        }
                        Type::Unknown
                    }
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    Type::Unknown
                }
            }
            Expr::MemberAssign(obj, field, value, name_span, span) => {
                let obj_ty = self.check_expr(*obj);
                let val_ty = self.check_expr(*value);

                let field_info = if let Type::Class(ref class_name) = obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        class_info.fields.get(&field).cloned()
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some((field_ty, field_span, doc)) = field_info {
                    if !self.is_assignable(&val_ty, &field_ty) {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                format!("{:?}", field_ty),
                                format!("{:?}", val_ty),
                            ),
                            span,
                        );
                    }
                    if self.record_node_info {
                        if let Some(d) = doc {
                            self.record_doc(name_span, d);
                        }
                        self.record_definition(name_span, self.current_file.clone(), field_span);
                        self.record_type(name_span, field_ty.clone());
                        self.record_type(span, field_ty.clone());
                    }
                } else {
                    if let Type::Class(class_name) = obj_ty {
                        if self.classes.contains_key(&class_name) {
                            self.error(
                                SemanticErrorKind::UndefinedField(class_name, field),
                                name_span,
                            );
                        } else {
                            self.error(SemanticErrorKind::UndefinedClass(class_name), span);
                        }
                    } else {
                        self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    }
                }
                val_ty
            }
            Expr::MethodCall(obj, method, name_span, args, span) => {
                let obj_ty = self.check_expr(*obj);
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                if let Type::Class(class_name) = obj_ty {
                    let method_info = if let Some(class_info) = self.classes.get(&class_name) {
                        if let Some(m) = class_info.methods.get(&method) {
                            Some(m.clone())
                        } else {
                            class_info.static_methods.get(&method).cloned()
                        }
                    } else {
                        None
                    };

                    if let Some((param_tys, ret_ty, doc, mspan)) = method_info {
                        if self.record_node_info {
                            if let Some(d) = doc {
                                self.record_doc(name_span, d.clone());
                            }
                            self.record_definition(name_span, self.current_file.clone(), mspan);
                            self.record_type(
                                name_span,
                                Type::Function(param_tys.clone(), Box::new(ret_ty.clone())),
                            );
                            self.record_type(span, ret_ty.clone());
                        }

                        if param_tys.len() != arg_tys.len() {
                            self.error(
                                SemanticErrorKind::WrongArgumentCount(
                                    method,
                                    param_tys.len(),
                                    arg_tys.len(),
                                ),
                                span,
                            );
                            return ret_ty;
                        }
                        for (i, arg_ty) in arg_tys.iter().enumerate() {
                            if !self.is_assignable(arg_ty, &param_tys[i]) {
                                self.error(
                                    SemanticErrorKind::TypeMismatch(
                                        format!("{:?}", param_tys[i]),
                                        format!("{:?}", arg_ty),
                                    ),
                                    span,
                                );
                            }
                        }
                        ret_ty
                    } else {
                        self.error(SemanticErrorKind::UndefinedMethod(class_name, method), span);
                        Type::Unknown
                    }
                } else if obj_ty == Type::String {
                    let ret_ty = match method.as_str() {
                        "len" => Type::Int32,
                        "charAt" => Type::String,
                        "substring" => Type::String,
                        "indexOf" => Type::Int32,
                        "toUpper" | "toLower" | "trim" => Type::String,
                        _ => {
                            self.error(
                                SemanticErrorKind::UndefinedMethod("string".to_string(), method),
                                span,
                            );
                            Type::Unknown
                        }
                    };
                    if self.record_node_info && !matches!(ret_ty, Type::Unknown) {
                        self.record_type(span, ret_ty.clone());
                    }
                    ret_ty
                } else if let Type::Array(inner) = obj_ty {
                    let ret_ty = match method.as_str() {
                        "len" => Type::Int32,
                        "push" => Type::Void,
                        "pop" => (*inner).clone(),
                        "join" => Type::String,
                        "get" => (*inner).clone(),
                        _ => {
                            self.error(
                                SemanticErrorKind::UndefinedMethod("array".to_string(), method),
                                span,
                            );
                            Type::Unknown
                        }
                    };
                    if self.record_node_info && !matches!(ret_ty, Type::Unknown) {
                        self.record_type(span, ret_ty.clone());
                    }
                    ret_ty
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    Type::Unknown
                }
            }
            Expr::This(span) => {
                if let Some(class_name) = &self.current_class {
                    Type::Class(class_name.clone())
                } else {
                    self.error(
                        SemanticErrorKind::UndefinedVariable("this".to_string()),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                self.check_expr(*expr);
                self.resolve_type(ty_expr);
                Type::Boolean
            }
            Expr::Throw(expr, span) => {
                let expr_ty = self.check_expr(*expr);
                let error_ty = Type::Class("Error".to_string());
                if !self.is_assignable(&expr_ty, &error_ty) {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "Error".to_string(),
                            format!("{:?}", expr_ty),
                        ),
                        span,
                    );
                }
                Type::Unknown // throw doesn't "return" a value to its context
            }
            Expr::Index(obj, index, span) => {
                let obj_ty = self.check_expr(*obj);
                let index_ty = self.check_expr(*index);
                if index_ty != Type::Int32 {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "i32".to_string(),
                            format!("{:?}", index_ty),
                        ),
                        span,
                    );
                }
                if let Type::Array(inner) = obj_ty {
                    *inner
                } else if obj_ty == Type::String {
                    Type::String
                } else {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "array or string".to_string(),
                            format!("{:?}", obj_ty),
                        ),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::UnaryOp(op, expr, span) => {
                let expr_ty = self.check_expr(*expr);
                if op == "-" && expr_ty.is_numeric() {
                    expr_ty
                } else {
                    self.error(
                        SemanticErrorKind::IncompatibleBinaryOperators(
                            format!("{:?}", expr_ty),
                            op,
                            "None".to_string(),
                        ),
                        span,
                    );
                    Type::Unknown
                }
            }
        };
        if self.record_node_info {
            self.record_type(span, ty.clone());
        }
        ty
    }
}
