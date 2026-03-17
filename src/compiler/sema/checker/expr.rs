use crate::compiler::ast::{AccessModifier, Expr, Span};
use crate::compiler::sema::checker::SemanticAnalyzer;
use crate::compiler::sema::checker::SemanticErrorKind;
use crate::compiler::sema::ty::Type;

impl SemanticAnalyzer {
    pub fn check_expr(&mut self, expr: Expr) -> Type {
        let span = expr.span();
        let ty = match expr {
            Expr::Number(_, _) => Type::Int32,
            Expr::Float(_, _) => Type::Float64,
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
                    Type::ClassType(name)
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
                    "+" | "-" | "*" | "/" | "%" => {
                        if lhs.is_numeric() && rhs.is_numeric() {
                            if lhs == Type::Float64 || rhs == Type::Float64 {
                                Type::Float64
                            } else if lhs == Type::Float32 || rhs == Type::Float32 {
                                Type::Float32
                            } else if lhs == Type::Int64 || rhs == Type::Int64 {
                                Type::Int64
                            } else {
                                Type::Int32
                            }
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
                    "&" | "|" | "^" | "<<" | ">>" => {
                        if lhs.is_integer() && rhs.is_integer() {
                            lhs
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
                let val_span = value.span();
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
                    } else if val_ty != expected_ty && self.record_node_info {
                        self.record_type(val_span, expected_ty.clone());
                    }
                    return expected_ty;
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                }
                val_ty
            }
            Expr::Call(name, type_args, name_span, args, span) => {
                let arg_spans: Vec<_> = args.iter().map(|a| a.span()).collect();
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                let _resolved_type_args: Vec<Type> = type_args
                    .into_iter()
                    .map(|ta| self.resolve_type(ta))
                    .collect();

                let sym = self.scope.lookup(&name);
                let function_ty = sym.map(|s| s.ty.clone());

                if let Some(Type::Function(type_params, param_tys, ret_ty)) = function_ty {
                    // TODO: Handle generic function instantiation if needed
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
                                Type::Function(
                                    type_params.clone(),
                                    param_tys.clone(),
                                    ret_ty.clone(),
                                ),
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
                        } else if arg_ty != &param_tys[i] && self.record_node_info {
                            self.record_type(arg_spans[i], param_tys[i].clone());
                        }
                    }
                    *ret_ty
                } else {
                    self.error(SemanticErrorKind::UndefinedFunction(name), name_span);
                    Type::Unknown
                }
            }
            Expr::New(class_name, type_args, name_span, args, span) => {
                let info_opt = self.classes.get(&class_name).cloned();
                if let Some(class_info) = info_opt {
                    if class_info.is_abstract {
                        self.error(
                            SemanticErrorKind::CannotInstantiateAbstractClass(class_name.clone()),
                            span,
                        );
                    }

                    let resolved_type_args: Vec<Type> = type_args
                        .into_iter()
                        .map(|ta| self.resolve_type(ta))
                        .collect();

                    if resolved_type_args.len() != class_info.type_params.len() {
                        self.error(
                            SemanticErrorKind::ArgumentCountMismatch(
                                class_info.type_params.len(),
                                resolved_type_args.len(),
                            ),
                            span,
                        );
                    }

                    let mut arg_tys = Vec::new();
                    for arg in &args {
                        arg_tys.push(self.check_expr(arg.clone()));
                    }

                    if let Some((params, _access)) = class_info.constructor.as_ref() {
                        let mapping = self
                            .get_substitution_mapping(&class_info.type_params, &resolved_type_args);
                        let substituted_params: Vec<Type> = params
                            .iter()
                            .map(|p| self.substitute(p, &mapping))
                            .collect();

                        if arg_tys.len() != substituted_params.len() {
                            self.error(
                                SemanticErrorKind::ArgumentCountMismatch(
                                    substituted_params.len(),
                                    arg_tys.len(),
                                ),
                                span,
                            );
                        } else {
                            for (i, arg_ty) in arg_tys.iter().enumerate() {
                                if !self.is_assignable(arg_ty, &substituted_params[i]) {
                                    self.error(
                                        SemanticErrorKind::TypeMismatch(
                                            substituted_params[i].to_string(),
                                            arg_ty.to_string(),
                                        ),
                                        args[i].span(),
                                    );
                                }
                            }
                        }
                    } else if !arg_tys.is_empty() {
                        self.error(
                            SemanticErrorKind::ArgumentCountMismatch(0, arg_tys.len()),
                            span,
                        );
                    }

                    let ret_ty = if resolved_type_args.is_empty() {
                        Type::Class(class_name.clone())
                    } else {
                        Type::Generic(class_name.clone(), resolved_type_args.clone())
                    };

                    if self.record_node_info {
                        let doc_opt = class_info.doc.clone();
                        let class_span = class_info.span;
                        if let Some(doc) = doc_opt {
                            self.record_doc(name_span, doc);
                        }
                        self.record_definition(name_span, self.current_file.clone(), class_span);
                        self.record_type(name_span, ret_ty.clone());
                        self.record_type(span, ret_ty.clone());
                    }
                    ret_ty
                } else {
                    self.error(SemanticErrorKind::UndefinedClass(class_name.clone()), span);
                    for arg in args {
                        self.check_expr(arg);
                    }
                    Type::Unknown
                }
            }
            Expr::MemberAccess(obj, field, name_span, span) => {
                let obj_ty = self.check_expr(*obj);

                if let Type::Enum(ref name) = obj_ty {
                    if let Some(members) = self.enums.get(name) {
                        if members.contains(&field) {
                            if self.record_node_info {
                                self.record_type(span, Type::Enum(name.clone()));
                            }
                            return Type::Enum(name.clone());
                        }
                    }
                    self.error(
                        SemanticErrorKind::UndefinedField(
                            format!("Enum(\"{}\")", name),
                            field.clone(),
                        ),
                        name_span,
                    );
                    return Type::Unknown;
                }

                if let Some((finfo, class_defined_in, class_span)) =
                    self.lookup_field_for_type(&obj_ty, &field)
                {
                    if let Type::Class(_) | Type::Generic(_, _) = obj_ty {
                        if finfo.is_static {
                            self.error(
                                SemanticErrorKind::AccessDenied(
                                    field.clone(),
                                    format!("{:?}", obj_ty),
                                    "instance".to_string(),
                                ),
                                name_span,
                            );
                        }
                    } else if let Type::ClassType(ref name) = obj_ty {
                        if !finfo.is_static {
                            self.error(
                                SemanticErrorKind::AccessDenied(
                                    field.clone(),
                                    name.clone(),
                                    "static".to_string(),
                                ),
                                name_span,
                            );
                        }
                    }
                    self.check_access(&finfo.defined_in_class, &field, finfo.access, name_span);

                    if self.record_node_info {
                        self.record_definition(name_span, class_defined_in, class_span);
                        self.record_type(name_span, finfo.ty.clone());
                        self.record_type(span, finfo.ty.clone());
                    }
                    return finfo.ty;
                } else {
                    let type_name = match obj_ty {
                        Type::Class(ref n) | Type::Generic(ref n, _) => n.clone(),
                        _ => format!("{:?}", obj_ty),
                    };
                    self.error(
                        SemanticErrorKind::UndefinedField(type_name, field),
                        name_span,
                    );
                    Type::Unknown
                }
            }
            Expr::MemberAssign(obj, field, value, name_span, span) => {
                let val_span = value.span();
                let obj_ty = self.check_expr(*obj.clone());
                let val_ty = self.check_expr(*value);

                if let Some((finfo, class_defined_in, class_span)) =
                    self.lookup_field_for_type(&obj_ty, &field)
                {
                    if let Type::Class(_) | Type::Generic(_, _) = obj_ty {
                        if finfo.is_static {
                            self.error(
                                SemanticErrorKind::AccessDenied(
                                    field.clone(),
                                    format!("{:?}", obj_ty),
                                    "instance".to_string(),
                                ),
                                name_span,
                            );
                        }
                    } else if let Type::ClassType(ref name) = obj_ty {
                        if !finfo.is_static {
                            self.error(
                                SemanticErrorKind::AccessDenied(
                                    field.clone(),
                                    name.clone(),
                                    "static".to_string(),
                                ),
                                name_span,
                            );
                        }
                    }
                    self.check_access(&finfo.defined_in_class, &field, finfo.access, name_span);

                    if finfo.is_readonly {
                        let mut allowed = false;
                        if let Expr::This(_) = *obj {
                            if self.current_method.as_deref() == Some("constructor")
                                && self.current_class.as_ref() == Some(&finfo.defined_in_class)
                            {
                                allowed = true;
                            }
                        }
                        if !allowed {
                            self.error(SemanticErrorKind::ReadonlyViolation(field.clone()), span);
                        }
                    }

                    if !self.is_assignable(&val_ty, &finfo.ty) {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                finfo.ty.to_string(),
                                val_ty.to_string(),
                            ),
                            span,
                        );
                    } else if val_ty != finfo.ty && self.record_node_info {
                        self.record_type(val_span, finfo.ty.clone());
                    }
                    if self.record_node_info {
                        self.record_definition(name_span, class_defined_in, class_span);
                        self.record_type(name_span, finfo.ty.clone());
                        self.record_type(span, finfo.ty.clone());
                    }
                    return finfo.ty;
                } else {
                    let type_name = match obj_ty {
                        Type::Class(ref n) | Type::Generic(ref n, _) => n.clone(),
                        _ => format!("{:?}", obj_ty),
                    };
                    self.error(
                        SemanticErrorKind::UndefinedField(type_name, field),
                        name_span,
                    );
                    Type::Unknown
                }
            }
            Expr::MethodCall(obj, method, type_args, name_span, args, span) => {
                let arg_spans: Vec<_> = args.iter().map(|a| a.span()).collect();
                let obj_ty = self.check_expr(*obj);
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                let _resolved_type_args: Vec<Type> = type_args
                    .into_iter()
                    .map(|ta| self.resolve_type(ta))
                    .collect();

                if let Some((minfo, class_defined_in, class_span)) =
                    self.lookup_method_for_type(&obj_ty, &method)
                {
                    if let Type::Class(_) | Type::Generic(_, _) = obj_ty {
                        if minfo.is_static {
                            self.error(
                                SemanticErrorKind::AccessDenied(
                                    method.clone(),
                                    format!("{:?}", obj_ty),
                                    "instance".to_string(),
                                ),
                                name_span,
                            );
                        }
                    } else if let Type::ClassType(ref name) = obj_ty {
                        if !minfo.is_static {
                            self.error(
                                SemanticErrorKind::AccessDenied(
                                    method.clone(),
                                    name.clone(),
                                    "static".to_string(),
                                ),
                                name_span,
                            );
                        }
                    }

                    self.check_access(&minfo.defined_in_class, &method, minfo.access, name_span);

                    if arg_tys.len() != minfo.params.len() {
                        self.error(
                            SemanticErrorKind::ArgumentCountMismatch(
                                minfo.params.len(),
                                arg_tys.len(),
                            ),
                            span,
                        );
                    } else {
                        for (i, arg_ty) in arg_tys.iter().enumerate() {
                            if !self.is_assignable(arg_ty, &minfo.params[i]) {
                                self.error(
                                    SemanticErrorKind::TypeMismatch(
                                        minfo.params[i].to_string(),
                                        arg_ty.to_string(),
                                    ),
                                    arg_spans[i],
                                );
                            } else if arg_ty != &minfo.params[i] && self.record_node_info {
                                self.record_type(arg_spans[i], minfo.params[i].clone());
                            }
                        }
                    }

                    if self.record_node_info {
                        self.record_definition(name_span, class_defined_in, class_span);
                        self.record_type(
                            name_span,
                            Type::Function(
                                minfo.type_params.clone(),
                                minfo.params.clone(),
                                Box::new(minfo.ret_ty.clone()),
                            ),
                        );
                        self.record_type(span, minfo.ret_ty.clone());
                    }
                    return minfo.ret_ty;
                }

                // Handle special types
                let ret_ty = match obj_ty {
                    Type::String => match method.as_str() {
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
                    },
                    Type::Array(ref inner) => match method.as_str() {
                        "len" => Type::Int32,
                        "push" => Type::Void,
                        "pop" => (**inner).clone(),
                        "join" => Type::String,
                        "get" => (**inner).clone(),
                        _ => {
                            self.error(
                                SemanticErrorKind::UndefinedMethod("array".to_string(), method),
                                span,
                            );
                            Type::Unknown
                        }
                    },
                    _ => {
                        let type_name = match obj_ty {
                            Type::Class(ref n)
                            | Type::Generic(ref n, _)
                            | Type::ClassType(ref n) => n.clone(),
                            _ => format!("{:?}", obj_ty),
                        };
                        self.error(SemanticErrorKind::UndefinedMethod(type_name, method), span);
                        Type::Unknown
                    }
                };

                if self.record_node_info && !matches!(ret_ty, Type::Unknown) {
                    self.record_type(span, ret_ty.clone());
                }
                ret_ty
            }
            Expr::This(span) => {
                if self.is_static_context {
                    self.error(SemanticErrorKind::ThisInStaticMethod, span);
                    return Type::Unknown;
                }
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
                } else if op == "~" && expr_ty.is_integer() {
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
            Expr::Super(span) => {
                if self.is_static_context {
                    self.error(SemanticErrorKind::SuperInStaticMethod, span);
                    return Type::Unknown;
                }
                if let Some(class_name) = &self.current_class {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if let Some(parent_expr) = &class_info.parent {
                            self.resolve_type(parent_expr.clone())
                        } else {
                            self.error(SemanticErrorKind::NoParentClass(class_name.clone()), span);
                            Type::Unknown
                        }
                    } else {
                        Type::Unknown
                    }
                } else {
                    self.error(SemanticErrorKind::SuperOutsideClass, span);
                    Type::Unknown
                }
            }
            Expr::SuperCall(args, span) => {
                let mut arg_tys = Vec::new();
                for arg in &args {
                    arg_tys.push(self.check_expr(arg.clone()));
                }

                if let Some(class_name) = &self.current_class {
                    if self.current_method.as_deref() != Some("constructor") {
                        // Allow super() only in constructor for now
                        // Actually super.method() is handled via MethodCall(Super, ...)
                        // super() is specifically for constructor
                    }

                    if let Some(class_info) = self.classes.get(class_name) {
                        if let Some(parent_expr) = &class_info.parent {
                            let parent_name = match parent_expr {
                                crate::compiler::ast::TypeExpr::Name(n, _) => Some(n.clone()),
                                crate::compiler::ast::TypeExpr::Generic(n, _, _) => Some(n.clone()),
                                _ => None,
                            };
                            if let Some(pn) = parent_name {
                                if let Some(parent_info) = self.classes.get(&pn) {
                                    let ctor_info = parent_info.constructor.clone();
                                    if let Some((param_tys, _)) = ctor_info {
                                        if param_tys.len() != arg_tys.len() {
                                            self.error(
                                                SemanticErrorKind::ArgumentCountMismatch(
                                                    param_tys.len(),
                                                    arg_tys.len(),
                                                ),
                                                span,
                                            );
                                        } else {
                                            for (i, arg_ty) in arg_tys.iter().enumerate() {
                                                if !self.is_assignable(arg_ty, &param_tys[i]) {
                                                    self.error(
                                                        SemanticErrorKind::TypeMismatch(
                                                            param_tys[i].to_string(),
                                                            arg_ty.to_string(),
                                                        ),
                                                        args[i].span(),
                                                    );
                                                } else if arg_ty != &param_tys[i]
                                                    && self.record_node_info
                                                {
                                                    self.record_type(
                                                        args[i].span(),
                                                        param_tys[i].clone(),
                                                    );
                                                }
                                            }
                                        }
                                    } else if !arg_tys.is_empty() {
                                        self.error(
                                            SemanticErrorKind::ArgumentCountMismatch(
                                                0,
                                                arg_tys.len(),
                                            ),
                                            span,
                                        );
                                    }
                                }
                            }
                        } else {
                            self.error(SemanticErrorKind::NoParentClass(class_name.clone()), span);
                        }
                    }
                } else {
                    self.error(SemanticErrorKind::SuperOutsideClass, span);
                }
                Type::Void
            }
        };
        if self.record_node_info {
            self.record_type(span, ty.clone());
        }
        ty
    }

    fn check_access(
        &mut self,
        class_name: &str,
        member_name: &str,
        access: AccessModifier,
        span: Span,
    ) {
        match access {
            AccessModifier::Public => {}
            AccessModifier::Private => {
                if self.current_class.as_deref() != Some(class_name) {
                    self.error(
                        SemanticErrorKind::AccessDenied(
                            member_name.to_string(),
                            class_name.to_string(),
                            "private".to_string(),
                        ),
                        span,
                    );
                }
            }
            AccessModifier::Protected => {
                if let Some(current) = &self.current_class {
                    if !self.is_assignable(
                        &Type::Class(current.clone()),
                        &Type::Class(class_name.to_string()),
                    ) {
                        self.error(
                            SemanticErrorKind::AccessDenied(
                                member_name.to_string(),
                                class_name.to_string(),
                                "protected".to_string(),
                            ),
                            span,
                        );
                    }
                } else {
                    self.error(
                        SemanticErrorKind::AccessDenied(
                            member_name.to_string(),
                            class_name.to_string(),
                            "protected".to_string(),
                        ),
                        span,
                    );
                }
            }
        }
    }
}
